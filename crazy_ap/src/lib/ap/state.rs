//! CAPWAP AP 纯状态机 —— 状态转换零副作用，可独立测试
//!
//! 优化后特性：
//! - 分状态独立超时（3s~90s），快速恢复
//! - 状态内重试（最多5次），超限才进 Failed
//! - AC 重传信号利用（收到重复响应 → 重发当前请求）
//! - 全局事件处理（EchoResponse/KeepAliveResponse 任何状态都能响应）
//! - 非预期事件打 warn 日志，提升可观测性

use std::time::Duration;
use tracing::{info, warn, debug};
use crate::capwap_packet::capwap_packet_build::WtpEventType;

// ============================================================================
// 状态与事件定义
// ============================================================================

/// AP 协议状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApState {
    Init,
    Discovery,
    Join,
    ConfigStatusRequest,
    ChangeStateRequest,
    DataCheck,
    Running,
    Online,
    Failed,
}

/// 触发状态转换的事件（由报文接收器产生）
#[derive(Debug, Clone)]
pub enum ApEvent {
    /// 收到 DiscoveryResponse
    DiscoveryResponse,
    /// 收到 JoinResponse
    JoinResponse,
    /// 收到 ConfigStatusResponse
    ConfigStatusResponse,
    /// 收到 ChangeStateResponse
    ChangeStateResponse,
    /// 收到 KeepAliveResponse
    KeepAliveResponse,
    /// 收到 EchoResponse
    EchoResponse,
    /// 收到 ConfigurationUpdateRequest（Running 期间 AC 下发配置）
    ConfigurationUpdateRequest { seq_number: u8 },
    /// 收到 IEEE 802.11 WTP Configuration Request
    Ieee80211ConfigRequest { seq_number: u8 },
    /// 收到 StationConfigurationRequest
    StationConfigurationRequest { seq_number: u8 },
    /// 收到 WtpEventResponse
    WtpEventResponse,
    /// 收到 ARP 请求（AC 在查询本 AP 的 MAC）
    ArpRequest {
        sender_mac: [u8; 6],
        sender_ip: [u8; 4],
        target_ip: [u8; 4],
    },
    /// ARP 回复已收到（AC MAC 解析成功）
    ArpResolved,
    /// 超时（当前状态内未收到有效报文）
    Timeout,
    /// 子任务失败（echo/keepalive 定时器重试耗尽）
    SubtaskFailed { name: &'static str },
}

/// 状态机产出的动作（由 Runner 执行）
#[derive(Debug, Clone)]
pub enum ApAction {
    /// 发送 CAPWAP 报文到 AC
    SendControl(Vec<u8>),
    /// 发送 Data 通道报文
    SendData(Vec<u8>),
    /// 发送 WtpEventRequest (STA 上线相关)
    SendWtpEvent {
        sta_mac: String,
        sta_ip: String,
        event_type: WtpEventType,
    },
    /// 发送配置响应（ConfigurationUpdate / IEEE80211 / StationConfig）
    SendConfigResponse {
        data: Vec<u8>,
    },
    /// 发送 ARP 应答
    SendArpReply {
        target_mac: [u8; 6],
        target_ip: [u8; 4],
    },
    /// 发送 ARP 请求（查询 AC 的 MAC，同时让 AC 学习本 AP 的 MAC）
    SendArpRequest,
    /// 通知 STA 可以上线
    NotifyStaOnline,
    /// 通知 keep-alive 发送器
    NotifyKeepAlive,
    /// 通知 echo 发送器
    NotifyEcho,
    /// 通知 STA 流程进入下一步
    NotifyStaNextStep,
    /// 延迟后重试（秒）
    DelayRetry { seconds: u64 },
    /// 无操作
    Noop,
}

// ============================================================================
// 状态机核心
// ============================================================================

/// 每个状态在进入 Failed 前的最大重试次数
const MAX_RETRIES_PER_STATE: u32 = 5;

pub struct ApStateMachine {
    pub state: ApState,
    pub index: u32,
    pub ap_mac: String,
    pub ap_ip: String,
    pub ap_serial: String,
    pub radio1_mac: String,
    pub radio2_mac: String,
    pub radio3_mac: Option<String>,
    /// 当前状态内的重试计数，成功推进状态时清零
    pub retry_count: u32,
}

impl ApStateMachine {
    pub fn new(
        index: u32,
        ap_mac: String,
        ap_ip: String,
        ap_serial: String,
        radio1_mac: String,
        radio2_mac: String,
        radio3_mac: Option<String>,
    ) -> Self {
        Self {
            state: ApState::Init,
            index,
            ap_mac,
            ap_ip,
            ap_serial,
            radio1_mac,
            radio2_mac,
            radio3_mac,
            retry_count: 0,
        }
    }

    /// 返回当前状态的超时时长（方案1：分状态独立超时）
    pub fn state_timeout(&self) -> Duration {
        match self.state {
            // 握手阶段：快速重试
            ApState::Init => Duration::from_secs(3),
            ApState::Discovery => Duration::from_secs(3),
            ApState::Join => Duration::from_secs(3),
            ApState::ConfigStatusRequest => Duration::from_secs(3),
            ApState::ChangeStateRequest => Duration::from_secs(3),
            ApState::DataCheck => Duration::from_secs(3),
            // 等待 AC 下发配置
            ApState::Running => Duration::from_secs(60),
            // 在线状态：定时器负责保活检测，此超时仅作安全兜底（5 分钟）
            ApState::Online => Duration::from_secs(300),
            ApState::Failed => Duration::from_secs(30),
        }
    }

    /// 统一日志标签：AP{index} {mac}
    fn label(&self) -> String {
        format!("AP{} {}", self.index, self.ap_mac)
    }

    /// 推进到新状态，清零重试计数
    fn advance(&mut self, next: ApState) {
        info!("{} {:?} → {:?}", self.label(), self.state, next);
        self.state = next;
        self.retry_count = 0;
    }

    /// 当前状态内重试计数+1，返回 true 表示超过上限应进 Failed
    fn check_retry(&mut self) -> bool {
        self.retry_count += 1;
        self.retry_count > MAX_RETRIES_PER_STATE
    }

    /// 核心方法：根据事件驱动状态转换，返回需要执行的动作列表
    ///
    /// 匹配优先级（从上到下）：
    /// 1. 正常流程转换
    /// 2. AC 重传信号（方案2）
    /// 3. 全局事件处理（方案4）
    /// 4. 分状态超时+重试（方案1）
    /// 5. Failed 恢复
    /// 6. 未预期组合日志（方案5）
    pub fn on_event(&mut self, event: ApEvent) -> Vec<ApAction> {
        match (&self.state, &event) {
            // ================================================================
            // 正常流程转换（高优先级）
            // ================================================================

            // Init: 收到 DiscoveryResponse → 发 Join
            (ApState::Init, ApEvent::DiscoveryResponse) => {
                self.advance(ApState::Discovery);
                let pkt = crate_join_wrapper(&self.ap_mac, &self.ap_ip, &self.ap_serial);
                vec![ApAction::SendControl(pkt)]
            }

            // Discovery: 收到 JoinResponse → 发 ConfigStatus
            (ApState::Discovery, ApEvent::JoinResponse) => {
                self.advance(ApState::Join);
                let pkt = crate_config_status_wrapper(&self.radio1_mac, &self.radio2_mac);
                vec![ApAction::SendControl(pkt)]
            }

            // Join: 收到 ConfigStatusResponse → 发 ChangeState
            (ApState::Join, ApEvent::ConfigStatusResponse) => {
                self.advance(ApState::ChangeStateRequest);
                let pkt = crate_change_state_wrapper();
                vec![ApAction::SendControl(pkt)]
            }

            // ChangeStateRequest: 收到 ChangeStateResponse → 发 KeepAlive
            (ApState::ChangeStateRequest, ApEvent::ChangeStateResponse) => {
                self.advance(ApState::DataCheck);
                let pkt = crate_keepalive_wrapper(&self.ap_mac);
                vec![ApAction::SendData(pkt)]
            }

            // DataCheck: 收到 KeepAliveResponse → 进入 Running
            (ApState::DataCheck, ApEvent::KeepAliveResponse) => {
                self.advance(ApState::Running);
                vec![ApAction::NotifyKeepAlive]
            }

            // Running: 收到 IEEE 802.11 Config → STA 上线
            (ApState::Running, ApEvent::Ieee80211ConfigRequest { seq_number }) => {
                self.advance(ApState::Online);
                let resp = crate_80211_config_response_wrapper(
                    &self.radio1_mac, &self.radio2_mac, *seq_number,
                );
                vec![
                    ApAction::SendConfigResponse { data: resp },
                    ApAction::NotifyStaOnline,
                    ApAction::NotifyEcho,
                ]
            }

            // Running: 收到 EchoResponse → AC 已就绪，推进到 Online
            (ApState::Running, ApEvent::EchoResponse) => {
                self.advance(ApState::Online);
                vec![ApAction::NotifyEcho]
            }

            // ================================================================
            // 方案2: AC 重传信号 → 重发当前阶段请求（状态不变）
            // AC 重复发某个响应，说明 AC 没收到我们的下一步请求
            // ================================================================

            // Discovery 状态再次收到 DiscoveryResponse → AC 在重传，我们重发 Join
            (ApState::Discovery, ApEvent::DiscoveryResponse) => {
                warn!("{} retransmit: DiscoveryResponse in Discovery, resending Join",
                    self.label());
                let pkt = crate_join_wrapper(&self.ap_mac, &self.ap_ip, &self.ap_serial);
                vec![ApAction::SendControl(pkt)]
            }

            // Join 状态再次收到 JoinResponse → AC 在重传，重发 ConfigStatus
            (ApState::Join, ApEvent::JoinResponse) => {
                warn!("{} retransmit: JoinResponse in Join, resending ConfigStatus",
                    self.label());
                let pkt = crate_config_status_wrapper(&self.radio1_mac, &self.radio2_mac);
                vec![ApAction::SendControl(pkt)]
            }

            // DataCheck 状态收到 ChangeStateResponse → AC 没收到 ChangeState
            (ApState::DataCheck, ApEvent::ChangeStateResponse) => {
                warn!("{} retransmit: ChangeStateResponse in DataCheck, resending ChangeState",
                    self.label());
                let pkt = crate_change_state_wrapper();
                vec![ApAction::SendControl(pkt)]
            }

            // ================================================================
            // 方案4: 全局事件处理（任何状态都能正确处理）
            // ================================================================

            // EchoResponse: 任何状态都刷新 Echo 定时器
            (_, ApEvent::EchoResponse) => {
                debug!("{} Echo OK", self.label());
                vec![ApAction::NotifyEcho]
            }

            // KeepAliveResponse: 任何状态都刷新 KeepAlive 定时器
            (_, ApEvent::KeepAliveResponse) => {
                debug!("{} KeepAlive OK", self.label());
                vec![ApAction::NotifyKeepAlive]
            }

            // ConfigurationUpdate: 任何状态都回复
            (_, ApEvent::ConfigurationUpdateRequest { seq_number }) => {
                info!("{} received ConfigurationUpdateRequest seq={}, sending response", self.label(), seq_number);
                let resp = crate_config_update_response_wrapper(*seq_number);
                vec![ApAction::SendConfigResponse { data: resp }]
            }
            // IEEE 802.11 Config: Running 已单独处理（推进到 Online）
            // Online 及其他状态 → 仅回复，状态不变
            (_, ApEvent::Ieee80211ConfigRequest { seq_number }) => {
                let resp = crate_80211_config_response_wrapper(
                    &self.radio1_mac, &self.radio2_mac, *seq_number,
                );
                vec![ApAction::SendConfigResponse { data: resp }]
            }
            // StationConfigurationRequest: 任何状态都回复
            (_, ApEvent::StationConfigurationRequest { seq_number }) => {
                debug!("{} received StationConfigurationRequest seq={}", self.label(), seq_number);
                let resp = crate_station_config_response_wrapper(*seq_number);
                vec![
                    ApAction::SendConfigResponse { data: resp },
                    ApAction::NotifyStaNextStep,
                ]
            }

            // WtpEventResponse: 任何状态都通知 STA 下一步
            (_, ApEvent::WtpEventResponse) => {
                vec![ApAction::NotifyStaNextStep]
            }

            // ARP 请求: 任何状态都回复（AC 查询本 AP 的 MAC）
            (_, ApEvent::ArpRequest { sender_mac, sender_ip, target_ip: _ }) => {
                vec![ApAction::SendArpReply {
                    target_mac: *sender_mac,
                    target_ip: *sender_ip,
                }]
            }

            // ================================================================
            // 方案1: 分状态超时 + 重试计数
            // ================================================================

            // ================================================================
            // Init: ARP 已解析 → 立即发 Discovery（动态 MAC）
            // ================================================================
            (ApState::Init, ApEvent::ArpResolved) => {
                info!("{} ARP resolved, sending Discovery (dynamic MAC)", self.label());
                let pkt = crate_discovery_wrapper(&self.ap_mac, &self.ap_serial);
                vec![ApAction::SendControl(pkt)]
            }

            // Init: 首次 Timeout → 只发 ARP（等待全局 receiver 通知 ArpResolved）
            (ApState::Init, ApEvent::Timeout) => {
                if self.retry_count == 0 {
                    // 首次：只发 ARP 请求
                    self.retry_count = 1;
                    debug!("{} sending ARP request, waiting for reply...", self.label());
                    vec![ApAction::SendArpRequest]
                } else if self.retry_count == 1 {
                    // ARP 超时 2s 未收到回复 → 用静态 MAC 发 Discovery
                    self.retry_count = 2;
                    warn!("{} ARP timeout, sending Discovery (static MAC)", self.label());
                    let pkt = crate_discovery_wrapper(&self.ap_mac, &self.ap_serial);
                    vec![ApAction::SendControl(pkt)]
                } else if self.check_retry() {
                    warn!("{} Init retry exhausted ({}) → Failed",
                        self.label(), MAX_RETRIES_PER_STATE);
                    self.state = ApState::Failed;
                    vec![]
                } else {
                    warn!("{} Init timeout (retry {}/{}), resending Discovery",
                        self.label(), self.retry_count, MAX_RETRIES_PER_STATE);
                    let pkt = crate_discovery_wrapper(&self.ap_mac, &self.ap_serial);
                    vec![ApAction::SendControl(pkt)]
                }
            }

            // Discovery: 超时 → 重发 Join（最多5次，耗尽进Failed，由runner 90s后重置）
            (ApState::Discovery, ApEvent::Timeout) => {
                if self.check_retry() {
                    warn!("{} Discovery retry exhausted ({}) → Failed",
                        self.label(), MAX_RETRIES_PER_STATE);
                    self.state = ApState::Failed;
                    vec![]
                } else {
                    warn!("{} Discovery timeout (retry {}/{}), resending Join",
                        self.label(), self.retry_count, MAX_RETRIES_PER_STATE);
                    let pkt = crate_join_wrapper(&self.ap_mac, &self.ap_ip, &self.ap_serial);
                    vec![ApAction::SendControl(pkt)]
                }
            }

            // Join: 超时 → 重发 ConfigStatus（最多5次，耗尽进Failed，由runner 90s后重置）
            (ApState::Join, ApEvent::Timeout) => {
                if self.check_retry() {
                    warn!("{} Join retry exhausted ({}) → Failed",
                        self.label(), MAX_RETRIES_PER_STATE);
                    self.state = ApState::Failed;
                    vec![]
                } else {
                    warn!("{} Join timeout (retry {}/{}), resending ConfigStatus",
                        self.label(), self.retry_count, MAX_RETRIES_PER_STATE);
                    let pkt = crate_config_status_wrapper(&self.radio1_mac, &self.radio2_mac);
                    vec![ApAction::SendControl(pkt)]
                }
            }

            // ChangeStateRequest: 超时 → 重发 ChangeState（最多5次，耗尽进Failed，由runner 90s后重置）
            (ApState::ChangeStateRequest, ApEvent::Timeout) => {
                if self.check_retry() {
                    warn!("{} ChangeStateRequest retry exhausted ({}) → Failed",
                        self.label(), MAX_RETRIES_PER_STATE);
                    self.state = ApState::Failed;
                    vec![]
                } else {
                    warn!("{} ChangeStateRequest timeout (retry {}/{}), resending ChangeState",
                        self.label(), self.retry_count, MAX_RETRIES_PER_STATE);
                    let pkt = crate_change_state_wrapper();
                    vec![ApAction::SendControl(pkt)]
                }
            }

            // DataCheck: 超时 → 重发 KeepAlive（最多5次，耗尽进Failed，由runner 90s后重置）
            (ApState::DataCheck, ApEvent::Timeout) => {
                if self.check_retry() {
                    warn!("{} DataCheck retry exhausted ({}) → Failed",
                        self.label(), MAX_RETRIES_PER_STATE);
                    self.state = ApState::Failed;
                    vec![]
                } else {
                    warn!("{} DataCheck timeout (retry {}/{}), resending KeepAlive",
                        self.label(), self.retry_count, MAX_RETRIES_PER_STATE);
                    let pkt = crate_keepalive_wrapper(&self.ap_mac);
                    vec![ApAction::SendData(pkt)]
                }
            }

            // Running: 超时 → Failed（等 IEEE802.11Config 太久，由runner 90s后重置）
            (ApState::Running, ApEvent::Timeout) => {
                warn!("{} Running timeout (no IEEE802.11Config for 60s) → Failed",
                    self.label());
                self.state = ApState::Failed;
                vec![]
            }

            // Online: 超时 → Failed（安全兜底，正常应由定时器重试耗尽触发）
            (ApState::Online, ApEvent::Timeout) => {
                warn!("{} Online timeout (fallback, 300s no events) → Failed",
                    self.label());
                self.state = ApState::Failed;
                vec![]
            }

            // ================================================================
            // 子任务报错（echo/keepalive 重试耗尽）
            // ================================================================
            (_, ApEvent::SubtaskFailed { name }) => {
                warn!("{} subtask '{}' retry exhausted → Failed", self.label(), name);
                self.state = ApState::Failed;
                vec![]
            }

            // ================================================================
            // 方案5: 未预期组合 → 打日志（提升可观测性）
            // ================================================================
            _ => {
                warn!("{} unexpected event {:?} in state {:?}",
                    self.label(), event, self.state);
                vec![ApAction::Noop]
            }
        }
    }
}

// ============================================================================
// 报文构造辅助函数（封装 capwap_packet_build 的调用）
// ============================================================================

use crate::capwap_packet::capwap_packet_build::{
    crate_change_state_request, crate_configuration_status_request,
    crate_configuration_update_response, crate_discovery,
    crate_keepalive, crate_join, crate_station_configuration_response,
    crate_80211_config_response,
};

fn crate_discovery_wrapper(mac: &str, serial: &str) -> Vec<u8> {
    crate_discovery(mac, serial).expect("Failed to build Discovery packet")
}

fn crate_join_wrapper(mac: &str, ip: &str, serial: &str) -> Vec<u8> {
    crate_join(mac, ip, serial).expect("Failed to build Join packet")
}

fn crate_config_status_wrapper(radio1: &str, radio2: &str) -> Vec<u8> {
    crate_configuration_status_request(radio1, radio2)
        .expect("Failed to build ConfigStatus packet")
}

fn crate_change_state_wrapper() -> Vec<u8> {
    crate_change_state_request().expect("Failed to build ChangeState packet")
}

fn crate_keepalive_wrapper(mac: &str) -> Vec<u8> {
    crate_keepalive(mac).expect("Failed to build KeepAlive packet")
}

fn crate_config_update_response_wrapper(seq: u8) -> Vec<u8> {
    crate_configuration_update_response(seq)
        .expect("Failed to build ConfigUpdateResponse")
}

fn crate_80211_config_response_wrapper(radio1: &str, radio2: &str, seq: u8) -> Vec<u8> {
    crate_80211_config_response(radio1, radio2, seq)
        .expect("Failed to build 80211ConfigResponse")
}

fn crate_station_config_response_wrapper(seq: u8) -> Vec<u8> {
    crate_station_configuration_response(seq)
        .expect("Failed to build StationConfigResponse")
}
