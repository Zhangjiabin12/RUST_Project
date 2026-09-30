//! AP 运行器（Raw Socket 版）
//!
//! 与旧版区别：
//! - 不再创建 per-AP UDP socket（发包走全局 RawSocket，收包走全局 receiver）
//! - 不再 spawn sender / ctrl_recv / data_recv task
//! - 每个 AP 减少 3 个 tokio task → 6000 AP 省 18000 task

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, Notify, Semaphore};
use tokio::task::JoinHandle;
use tokio::time::{sleep, timeout};
use tracing::{debug, error, info, warn};

use super::state::{ApAction, ApEvent, ApState, ApStateMachine};
use crate::ap::types::StaConfig;
use crate::network::raw_socket::RawSocket;
use crate::infra::util;

// ============================================================================
// 保活共享状态
// ============================================================================

/// Echo / KeepAlive 共享的重试状态
#[derive(Clone)]
pub struct KeepaliveState {
    /// EchoResponse 是否在上一个发送间隔内收到
    pub echo_responded: Arc<AtomicBool>,
    /// KeepAliveResponse 是否在上一个发送间隔内收到
    pub ka_responded: Arc<AtomicBool>,
    /// Echo/KeepAlive 公用失败计数（任一未收到都 +1，任一收到都清零）
    pub retry_count: Arc<AtomicU32>,
}

impl KeepaliveState {
    pub fn new() -> Self {
        Self {
            echo_responded: Arc::new(AtomicBool::new(false)),
            ka_responded: Arc::new(AtomicBool::new(false)),
            retry_count: Arc::new(AtomicU32::new(0)),
        }
    }
}

// ============================================================================
// ApRunner
// ============================================================================

pub struct ApRunner {
    pub index: u32,
    pub sm: ApStateMachine,
    pub stas: Vec<StaConfig>,

    pub raw_sock: Arc<RawSocket>,
    ap_mac: [u8; 6],
    ap_ip: [u8; 4],
    ctrl_port: u16,
    pub data_port: u16,
    pub sta_timeout_secs: u64,
    pub sta_max_retry: u32,
    pub sta_retry_interval_secs: u64,

    // 保活
    pub ka_state: KeepaliveState,
    pub ka_echo_interval_secs: u64,
    pub ka_ka_interval_secs: u64,
    pub ka_max_retry: u32,

    // STA 同步原语
    sta_sem: Arc<Semaphore>,
    step_nfy: Arc<tokio::sync::Notify>,
    /// ARP 解析通知（全局 receiver 解析到 AC MAC 后通知所有 AP）
    arp_nfy: Arc<tokio::sync::Notify>,
}

impl ApRunner {
    pub fn new(
        index: u32,
        ap_mac: String,
        ap_ip: String,
        ap_serial: String,
        radio1_mac: String,
        radio2_mac: String,
        radio3_mac: Option<String>,
        stas: Vec<StaConfig>,
        raw_sock: Arc<RawSocket>,
        arp_nfy: Arc<tokio::sync::Notify>,
        sta_timeout_secs: u64,
        sta_max_retry: u32,
        sta_retry_interval_secs: u64,
        ka_echo_interval_secs: u64,
        ka_ka_interval_secs: u64,
        ka_max_retry: u32,
    ) -> Self {
        let mac_bytes = util::parse_mac_hex(&ap_mac);
        let ip_bytes = util::parse_ip_hex(&ap_ip);
        let ctrl_port = 10000 + (index % 50000) as u16;
        let data_port = ctrl_port + 1;

        let ka_state = KeepaliveState::new();
        let sta_sem = Arc::new(Semaphore::new(0));
        let step_nfy = Arc::new(tokio::sync::Notify::new());

        Self {
            index,
            sm: ApStateMachine::new(index, ap_mac, ap_ip, ap_serial, radio1_mac, radio2_mac, radio3_mac),
            stas,
            raw_sock,
            ap_mac: mac_bytes,
            ap_ip: ip_bytes,
            ctrl_port,
            data_port,
            ka_state,
            ka_echo_interval_secs,
            ka_ka_interval_secs,
            ka_max_retry,
            sta_sem,
            step_nfy,
            arp_nfy,
            sta_timeout_secs,
            sta_max_retry,
            sta_retry_interval_secs,
        }
    }

    /// 主入口 —— 动态超时 + 子任务健康监控
    pub async fn run(mut self, mut event_rx: mpsc::Receiver<ApEvent>) -> anyhow::Result<()> {
        info!("AP{} start Online", self.index);

        // ── 通道 ──
        let (tun_tx, tun_rx) = mpsc::channel::<TunnelMsg>(1000);
        let tun_rx = Arc::new(tokio::sync::Mutex::new(tun_rx));

        // ── Clone 同步原语给子任务 ──
        let ka_state = self.ka_state.clone();
        let sem = self.sta_sem.clone();
        let step_nfy = self.step_nfy.clone();

        // ── 方案3: 子任务错误通道（用于主循环监控子任务健康）──
        let (sub_err_tx, mut sub_err_rx) = mpsc::channel::<&'static str>(4);
        let ka_echo_interval = Duration::from_secs(self.ka_echo_interval_secs);
        let ka_ka_interval = Duration::from_secs(self.ka_ka_interval_secs);
        let ka_max_retry = self.ka_max_retry;

        // ── 方案3: spawn 子任务并包装错误上报 ──
        let ka_handle = {
            let err_tx = sub_err_tx.clone();
            let index = self.index;
            let mac = self.ap_mac;
            let ip = self.ap_ip;
            let sport = self.data_port;
            let sock = self.raw_sock.clone();
            let state = ka_state.clone();
            tokio::spawn(async move {
                if let Err(e) = run_ka_timer(index, mac, ip, sport, sock, state, ka_ka_interval, ka_max_retry).await {
                    error!("ka subtask error: {}", e);
                    let _ = err_tx.send("keepalive").await;
                }
            })
        };

        let echo_handle = {
            let err_tx = sub_err_tx.clone();
            let index = self.index;
            let mac = self.ap_mac;
            let ip = self.ap_ip;
            let sport = self.ctrl_port;
            let sock = self.raw_sock.clone();
            tokio::spawn(async move {
                if let Err(e) = run_echo_timer(index, mac, ip, sport, sock, ka_state, ka_echo_interval, ka_max_retry).await {
                    error!("echo subtask error: {}", e);
                    let _ = err_tx.send("echo").await;
                }
            })
        };

        // ── 其余子任务 ──
        let mut tasks = TaskSet::new();
        tasks.spawn(run_tun_fwd(
            tun_rx, self.raw_sock.clone(),
            self.ap_mac, self.ap_ip, self.data_port, self.ctrl_port,
            step_nfy.clone(), sem.clone(),
            self.sta_timeout_secs, self.sta_max_retry, self.sta_retry_interval_secs,
        ));
        tasks.spawn(run_sta_launch(self.index, self.stas.clone(), sem, tun_tx));

        sleep(Duration::from_millis(50)).await;

        // ── 首次驱动 ──
        // 后续 AP：先发 ARP（让 AC 学到本 AP 的 MAC），再立即触发 Discovery
        let already_resolved = self.raw_sock.resolved_dst_mac.lock().unwrap().is_some();
        if already_resolved {
            debug!("AP{} AC MAC already resolved, send ARP then Discovery after 50µs", self.index);
            let acts = self.sm.on_event(ApEvent::Timeout); // retry_count=0 → SendArpRequest
            self.exec(&acts).await;
            sleep(Duration::from_nanos(50)).await;        // 给 AC 一点时间处理 ARP
        }
        let first_event = if already_resolved {
            ApEvent::ArpResolved // 跳过等待，直接发 Discovery
        } else {
            ApEvent::Timeout     // 正常流程：等 ARP 回复或 2s 超时
        };
        let acts = self.sm.on_event(first_event);
        self.exec(&acts).await;

        // ── 方案1+3: 事件循环 —— 动态超时 + 子任务监控 ──
        loop {
            let timeout_dur = self.sm.state_timeout();
            // AC MAC 未解析时，额外监听 ARP 通知
            let waiting_arp = self.raw_sock.resolved_dst_mac.lock().unwrap().is_none();

            let ev = if waiting_arp {
                tokio::select! {
                    e = event_rx.recv() => {
                        match e {
                            Some(ev) => ev,
                            None => break,
                        }
                    }
                    _ = sleep(timeout_dur) => {
                        ApEvent::Timeout
                    }
                    name = sub_err_rx.recv() => {
                        let task_name = name.unwrap_or("unknown");
                        error!("AP{} subtask '{}' failed, triggering recovery",
                            self.index, task_name);
                        ApEvent::SubtaskFailed { name: task_name }
                    }
                    // ARP 通知：全局 receiver 解析到 AC MAC 后唤醒
                    _ = self.arp_nfy.notified() => {
                        ApEvent::ArpResolved
                    }
                }
            } else {
                tokio::select! {
                    e = event_rx.recv() => {
                        match e {
                            Some(ev) => ev,
                            None => break,
                        }
                    }
                    _ = sleep(timeout_dur) => {
                        ApEvent::Timeout
                    }
                    name = sub_err_rx.recv() => {
                        let task_name = name.unwrap_or("unknown");
                        error!("AP{} subtask '{}' failed, triggering recovery",
                            self.index, task_name);
                        ApEvent::SubtaskFailed { name: task_name }
                    }
                }
            };

            let acts = self.sm.on_event(ev);
            self.exec(&acts).await;

            if self.sm.state == ApState::Failed {
                error!("AP{} failed, restarting", self.index);
                ka_handle.abort();
                echo_handle.abort();
                tasks.abort_all();
                return Err(anyhow::anyhow!("AP{} failed", self.index));
            }
        }

        ka_handle.abort();
        echo_handle.abort();
        tasks.abort_all();
        Ok(())
    }

    /// 执行动作 —— 直接调 raw_socket.send()，失败打日志（方案5）
    async fn exec(&self, acts: &[ApAction]) {
        for a in acts {
            match a {
                ApAction::SendControl(data) => {
                    if let Err(e) = self.raw_sock.send(
                        self.ap_mac, self.ap_ip, self.ctrl_port, 5246, data,
                    ) {
                        warn!("AP{} SendControl failed: {}", self.index, e);
                    }
                }
                ApAction::SendData(data) => {
                    if let Err(e) = self.raw_sock.send(
                        self.ap_mac, self.ap_ip, self.data_port, 5247, data,
                    ) {
                        warn!("AP{} SendData failed: {}", self.index, e);
                    }
                }
                ApAction::SendConfigResponse { data } => {
                    if let Err(e) = self.raw_sock.send(
                        self.ap_mac, self.ap_ip, self.ctrl_port, 5246, data,
                    ) {
                        warn!("AP{} SendConfigResponse failed: {}", self.index, e);
                    } else {
                        debug!("AP{} SendConfigResponse {}B to 5246", self.index, data.len());
                    }
                }
                ApAction::SendArpReply { target_mac, target_ip } => {
                    if let Err(e) = self.raw_sock.send_arp_reply(
                        self.ap_mac, self.ap_ip, *target_mac, *target_ip,
                    ) {
                        warn!("AP{} SendArpReply failed: {}", self.index, e);
                    }
                }
                ApAction::SendArpRequest => {
                    if let Err(e) = self.raw_sock.send_arp_request(
                        self.ap_mac, self.ap_ip, self.raw_sock.dst_ip,
                    ) {
                        warn!("AP{} SendArpRequest failed: {}", self.index, e);
                    } else {
                        debug!("AP{} sent ARP: who-has {:02x?} tell {:02x?}",
                            self.index, self.raw_sock.dst_ip, self.ap_ip);
                    }
                }
                ApAction::SendWtpEvent { .. } => {}
                ApAction::NotifyStaOnline => {
                    // +2: 1 个给 run_sta_launch 的初始 acquire，1 个给第一个 STA
                    self.sta_sem.add_permits(2);
                }
                ApAction::NotifyKeepAlive => {
                    self.ka_state.ka_responded.store(true, Ordering::SeqCst);
                }
                ApAction::NotifyEcho => {
                    self.ka_state.echo_responded.store(true, Ordering::SeqCst);
                }
                ApAction::NotifyStaNextStep => {
                    self.step_nfy.notify_one();
                }
                ApAction::DelayRetry { seconds } => {
                    sleep(Duration::from_secs(*seconds)).await;
                }
                ApAction::Noop => {}
            }
        }
    }
}

// ============================================================================
// 类型别名
// ============================================================================

type TunnelMsg = (Vec<u8>, String, String, String);

// ============================================================================
// 子任务：Echo / KeepAlive 定时发送（定时触发 + 共用重试计数）
// ============================================================================

/// KeepAlive 定时器：Data 通道 (5247)
///
/// 逻辑：
/// - 立即发送第一个 KeepAlive
/// - 每隔 interval 再次发送
/// - 发送前检查上一轮是否收到 KeepAliveResponse：
///   - 收到 → retry 清零
///   - 未收到 → retry++，若 > max_retry → 报错退出
async fn run_ka_timer(
    index: u32, mac: [u8; 6], ip: [u8; 4], sport: u16,
    sock: Arc<RawSocket>, state: KeepaliveState,
    interval: Duration, max_retry: u32,
) -> anyhow::Result<()> {
    let label = format!("AP{} {}", index, util::mac_bytes_to_hex(&mac));

    // 首次立即发送
    let payload = crate::capwap_packet::capwap_packet_build::build_keepalive_raw(&mac);
    if let Err(e) = sock.send(mac, ip, sport, 5247, &payload) {
        warn!("{} ka_timer send failed: {}", label, e);
    }

    loop {
        sleep(interval).await;

        let responded = state.ka_responded.swap(false, Ordering::SeqCst);
        if responded {
            state.retry_count.store(0, Ordering::SeqCst);
        } else {
            let retries = state.retry_count.fetch_add(1, Ordering::SeqCst) + 1;
            warn!("{} ka_timer no response (retry {}/{})", label, retries, max_retry);
            if retries > max_retry {
                return Err(anyhow::anyhow!("{} ka retry exhausted ({}/{})", label, retries, max_retry));
            }
        }

        let payload = crate::capwap_packet::capwap_packet_build::build_keepalive_raw(&mac);
        if let Err(e) = sock.send(mac, ip, sport, 5247, &payload) {
            warn!("{} ka_timer send failed: {}", label, e);
        }
    }
}

/// Echo 定时器：Control 通道 (5246)
///
/// 逻辑与 ka_timer 相同，共用 retry_count
async fn run_echo_timer(
    index: u32, mac: [u8; 6], ip: [u8; 4], sport: u16,
    sock: Arc<RawSocket>, state: KeepaliveState,
    interval: Duration, max_retry: u32,
) -> anyhow::Result<()> {
    let label = format!("AP{} {}", index, util::mac_bytes_to_hex(&mac));

    // 首次立即发送
    let payload = crate::capwap_packet::capwap_packet_build::build_echo_raw(&mac);
    if let Err(e) = sock.send(mac, ip, sport, 5246, &payload) {
        warn!("{} echo_timer send failed: {}", label, e);
    }

    loop {
        sleep(interval).await;

        let responded = state.echo_responded.swap(false, Ordering::SeqCst);
        if responded {
            state.retry_count.store(0, Ordering::SeqCst);
        } else {
            let retries = state.retry_count.fetch_add(1, Ordering::SeqCst) + 1;
            warn!("{} echo_timer no response (retry {}/{})", label, retries, max_retry);
            if retries > max_retry {
                return Err(anyhow::anyhow!("{} echo retry exhausted ({}/{})", label, retries, max_retry));
            }
        }

        let payload = crate::capwap_packet::capwap_packet_build::build_echo_raw(&mac);
        if let Err(e) = sock.send(mac, ip, sport, 5246, &payload) {
            warn!("{} echo_timer send failed: {}", label, e);
        }
    }
}

async fn run_tun_fwd(
    rx: Arc<tokio::sync::Mutex<mpsc::Receiver<TunnelMsg>>>,
    sock: Arc<RawSocket>,
    mac: [u8; 6], ip: [u8; 4], data_port: u16, ctrl_port: u16,
    step_nfy: Arc<Notify>, sta_sem: Arc<Semaphore>,
    sta_timeout_secs: u64, sta_max_retry: u32, sta_retry_interval_secs: u64,
) -> anyhow::Result<()> {
    use crate::capwap_packet::capwap_packet_build::{crate_tunnel_message, WtpEventType};

    while let Some((msg, sta_mac, sta_ip, radio)) = rx.lock().await.recv().await {
        let mut retry = 0u32;

        'sta_retry: loop {
            if retry >= sta_max_retry {
                warn!("tun_fwd STA {} retry exhausted ({}/{}), skipping",
                    sta_mac, retry, sta_max_retry);
                sta_sem.add_permits(1);
                break 'sta_retry;
            }
            if retry > 0 {
                warn!("tun_fwd STA {} retry {}/{}", sta_mac, retry, sta_max_retry);
                sleep(Duration::from_secs(sta_retry_interval_secs)).await;
            }

            let tunnel = crate_tunnel_message(msg.clone(), &radio).unwrap();
            if let Err(e) = sock.send(mac, ip, data_port, 5247, &tunnel) {
                warn!("tun_fwd tunnel send failed: {}", e);
            }

            // 等 AC 回复 StationConfigurationRequest
            if timeout(Duration::from_secs(sta_timeout_secs), step_nfy.notified()).await.is_err() {
                warn!("tun_fwd STA {} timeout StationConfigRequest (step1)", sta_mac);
                retry += 1;
                continue 'sta_retry;
            }
            let ev1 = crate::capwap_packet::capwap_packet_build::build_wtp_event(&sta_mac, &sta_ip, WtpEventType::ApInfoGatherReport);
            if let Err(e) = sock.send(mac, ip, ctrl_port, 5246, &ev1) {
                warn!("tun_fwd wtp_event1 send failed: {}", e);
            }

            if timeout(Duration::from_secs(sta_timeout_secs), step_nfy.notified()).await.is_err() {
                warn!("tun_fwd STA {} timeout WtpEventResponse (step2)", sta_mac);
                retry += 1;
                continue 'sta_retry;
            }
            let ev2 = crate::capwap_packet::capwap_packet_build::build_wtp_event(&sta_mac, &sta_ip, WtpEventType::StaIpReport);
            if let Err(e) = sock.send(mac, ip, ctrl_port, 5246, &ev2) {
                warn!("tun_fwd wtp_event2 send failed: {}", e);
            }

            if timeout(Duration::from_secs(sta_timeout_secs), step_nfy.notified()).await.is_err() {
                warn!("tun_fwd STA {} timeout WtpEventResponse (step3)", sta_mac);
                retry += 1;
                continue 'sta_retry;
            }

            // 成功
            sta_sem.add_permits(1);
            break 'sta_retry;
        }
    }
    Ok(())
}

async fn run_sta_launch(
    index: u32, stas: Vec<StaConfig>,
    sem: Arc<Semaphore>, ath_tx: mpsc::Sender<TunnelMsg>,
) -> anyhow::Result<()> {
    let _ = sem.acquire().await;
    sleep(Duration::from_secs(3)).await;

    for sta in &stas {
        let _ = sem.acquire().await;
        sem.forget_permits(1);
        sleep(Duration::from_millis(10)).await;

        let assoc = crate::capwap_packet::capwap_packet_build::crate_association_request(
            &sta.ap_radio_mac, &sta.mac, &sta.ap_radio_mac,
        ).unwrap();
        // info!("AP{} STA{} assoc: sta_mac={}, bssid={}", index, sta.sta_index, sta.mac, sta.ap_radio_mac);
        let _ = ath_tx.send((assoc, sta.mac.clone(), sta.ip.clone(), sta.ap_radio_mac.clone())).await;
        debug!("AP{} STA {} online", index, sta.mac);
    }
    info!("AP{} all STAs online", index);
    Ok(())
}

// ============================================================================
// TaskSet
// ============================================================================

struct TaskSet {
    handles: Vec<JoinHandle<anyhow::Result<()>>>,
}

impl TaskSet {
    fn new() -> Self { Self { handles: vec![] } }
    fn spawn(&mut self, fut: impl std::future::Future<Output = anyhow::Result<()>> + Send + 'static) {
        self.handles.push(tokio::spawn(fut));
    }
    fn abort_all(&mut self) {
        for h in self.handles.drain(..) { h.abort(); }
    }
}

impl Drop for TaskSet {
    fn drop(&mut self) {
        for h in &self.handles { h.abort(); }
    }
}
