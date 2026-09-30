//! CAPWAP 报文解析辅助 —— 轻量级解析，只返回消息类型，不修改状态
//!
//! 与旧 capwap_packet_parse.rs 的区别：
//! - 旧版：解析 + 修改 AP 状态 + 发送响应（耦合严重）
//! - 新版：只解析报文类型，状态变更由状态机管理

use crate::capwap_packet::capwap_packet_build::{CapwapHeader, CapwapPreamble};

/// 解析后的报文类型
#[derive(Debug, Clone, PartialEq)]
pub enum ParsedMessage {
    DiscoveryResponse,
    JoinResponse,
    ConfigStatusResponse,
    ChangeStateResponse,
    ConfigurationUpdateRequest { seq_number: u8 },
    Ieee80211ConfigRequest { seq_number: u8 },
    StationConfigurationRequest { seq_number: u8 },
    WtpEventResponse,
    EchoResponse,
    KeepAliveResponse,
    EchoRequest,
    Unknown,
}

impl ParsedMessage {
    /// 从 CAPWAP 载荷直接解析（不需要 Eth/IP/UDP 头）
    pub fn from_raw(data: &[u8]) -> Self {
        parse_message_type(data)
    }
}

/// 快速解析 CAPWAP 报文，只判断消息类型
/// 不做状态修改，不做响应发送
pub fn parse_message_type(data: &[u8]) -> ParsedMessage {
    if data.is_empty() {
        return ParsedMessage::Unknown;
    }

    let preamble = match CapwapPreamble::from_bytes(data) {
        Ok(p) => p,
        Err(_) => return ParsedMessage::Unknown,
    };

    let data = &data[preamble.get_data_length() as usize..];
    let header = match CapwapHeader::from_bytes(data) {
        Ok(h) => h,
        Err(_) => return ParsedMessage::Unknown,
    };

    // Keep-Alive（Data 通道，header 中 K 标志位 = 1），不分 preamble type
    if header.keep_alive == 1 {
        return ParsedMessage::KeepAliveResponse;
    }

    // Data 通道非 KeepAlive 报文（隧道数据等）：静默跳过
    if preamble.type_ == 2 {
        return ParsedMessage::Unknown;
    }

    let data = &data[header.get_data_length() as usize..];
    let control_header = match crate::capwap_packet::capwap_packet_build::CapwapControlHeader::from_bytes(data) {
        Ok(h) => h,
        Err(_) => return ParsedMessage::Unknown,
    };

    let full_type = ((control_header.message_type_reserved as u32) << 8) | (control_header.message_type as u32);
    let seq = control_header.sequence_number;

    match full_type {
        2 => ParsedMessage::DiscoveryResponse,
        4 => ParsedMessage::JoinResponse,
        6 => ParsedMessage::ConfigStatusResponse,
        12 => ParsedMessage::ChangeStateResponse,
        7 => ParsedMessage::ConfigurationUpdateRequest { seq_number: seq },
        3398913 => ParsedMessage::Ieee80211ConfigRequest { seq_number: seq },
        25 => ParsedMessage::StationConfigurationRequest { seq_number: seq },
        10 => ParsedMessage::WtpEventResponse,
        14 => ParsedMessage::EchoResponse,
        13 => ParsedMessage::EchoRequest,
        _ => {
            tracing::debug!("unknown CAPWAP msg type={} full_type={} seq={}",
                control_header.message_type, full_type, seq);
            ParsedMessage::Unknown
        }
    }
}
