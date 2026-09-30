//! 通用类型定义

/// STA 终端配置
#[derive(Debug, PartialEq, Clone)]
pub struct StaConfig {
    pub ap_index: u32,
    pub sta_index: u32,
    pub mac: String,
    pub ip: String,
    pub ap_radio_mac: String,
}
