use serde::Deserialize;
use std::fs;

/// TOML 配置结构（匹配 config/wtp_con 的节结构）
#[derive(Debug, Deserialize)]
struct TomlConfig {
    thread: Option<TomlThread>,
    ap_batch: Option<TomlApBatch>,
    ac: Option<TomlAc>,
    ssid_mode: Option<TomlSsidMode>,
    socket: Option<TomlSocket>,
    ap: Option<TomlAp>,
    sta: Option<TomlSta>,
    keepalive: Option<TomlKeepalive>,
}

#[derive(Debug, Deserialize)]
struct TomlThread {
    interval: Option<f32>,
}

#[derive(Debug, Deserialize)]
struct TomlApBatch {
    batch_size: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct TomlAc {
    mac: Option<String>,
    backup_mac: Option<String>,
    backup_enable: Option<u32>,
    dest_ip: Option<String>,
    dest_ip_backup: Option<String>,
    control_port: Option<u32>,
    data_port: Option<u32>,
    iface_name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TomlSsidMode {
    ssid: Option<String>,
    mode: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TomlSocket {
    rmem_max: Option<u32>,
    wmem_max: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct TomlAp {
    nums: Option<u32>,
    mac_prefix: Option<String>,
    ip_prefix: Option<String>,
    serial_prefix: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TomlSta {
    count: Option<u32>,
    mac_prefix: Option<String>,
    ip_prefix: Option<String>,
    timeout_secs: Option<u64>,
    max_retry: Option<u32>,
    retry_interval_secs: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct TomlKeepalive {
    echo_interval_secs: Option<u64>,
    ka_interval_secs: Option<u64>,
    max_retry: Option<u32>,
}

// ── 对外接口：保持扁平 ConfigData，兼容现有调用 ──

#[derive(Debug)]
pub struct ConfigData {
    pub ap_thread_interval: f32,
    pub ap_batch_size: u32,
    pub ac_mac: String,
    pub ac_backup_mac: String,
    pub backup_enable: u32,
    pub dest_ip: String,
    pub dest_ip_backup: String,
    pub control_port: u32,
    pub data_port: u32,
    pub iface_name: String,
    pub ssid: String,
    pub mode: String,
    pub ap_nums: u32,
    pub sta_count: u32,
    pub ap_mac_prefix: String,
    pub ap_ip_prefix: String,
    pub ap_serial_prefix: String,
    pub sta_mac_prefix: String,
    pub sta_ip_prefix: String,
    pub sta_timeout_secs: u64,
    pub sta_max_retry: u32,
    pub sta_retry_interval_secs: u64,
    pub socket_rmem: u32,
    pub socket_wmem: u32,
    pub ka_echo_interval_secs: u64,
    pub ka_ka_interval_secs: u64,
    pub ka_max_retry: u32,
}

pub fn read_user_config(config_path: &str) -> Result<ConfigData, Box<dyn std::error::Error>> {
    let contents = fs::read_to_string(config_path)?;
    let tc: TomlConfig = toml::from_str(&contents)?;

    // 校验序列号前缀长度必须是偶数（hex 解码要求）
    let serial_prefix = tc.ap.as_ref()
        .and_then(|a| a.serial_prefix.clone())
        .unwrap_or_else(|| "27003809000000".into());
    if serial_prefix.len() % 2 != 0 {
        return Err(format!(
            "ap.serial_prefix length must be even (got {} chars: \"{}\")",
            serial_prefix.len(), serial_prefix
        ).into());
    }

    Ok(ConfigData {
        ap_thread_interval: tc.thread.as_ref().and_then(|t| t.interval).unwrap_or(0.5),
        ap_batch_size: tc.ap_batch.as_ref().and_then(|b| b.batch_size).unwrap_or(1),
        ac_mac: tc.ac.as_ref().and_then(|a| a.mac.clone()).unwrap_or_default(),
        ac_backup_mac: tc.ac.as_ref().and_then(|a| a.backup_mac.clone()).unwrap_or_default(),
        backup_enable: tc.ac.as_ref().and_then(|a| a.backup_enable).unwrap_or(0),
        dest_ip: tc.ac.as_ref().and_then(|a| a.dest_ip.clone()).unwrap_or_default(),
        dest_ip_backup: tc.ac.as_ref().and_then(|a| a.dest_ip_backup.clone()).unwrap_or_default(),
        control_port: tc.ac.as_ref().and_then(|a| a.control_port).unwrap_or(5246),
        data_port: tc.ac.as_ref().and_then(|a| a.data_port).unwrap_or(5247),
        iface_name: tc.ac.as_ref().and_then(|a| a.iface_name.clone()).unwrap_or_default(),
        ssid: tc.ssid_mode.as_ref().and_then(|s| s.ssid.clone()).unwrap_or_default(),
        mode: tc.ssid_mode.as_ref().and_then(|s| s.mode.clone()).unwrap_or_default(),
        ap_nums: tc.ap.as_ref().and_then(|a| a.nums).unwrap_or(0),
        ap_mac_prefix: tc.ap.as_ref().and_then(|a| a.mac_prefix.clone()).unwrap_or_else(|| "ccd81f".into()),
        ap_ip_prefix: tc.ap.as_ref().and_then(|a| a.ip_prefix.clone()).unwrap_or_else(|| "0200".into()),
        ap_serial_prefix: serial_prefix,
        sta_count: tc.sta.as_ref().and_then(|s| s.count).unwrap_or(0),
        sta_mac_prefix: tc.sta.as_ref().and_then(|s| s.mac_prefix.clone()).unwrap_or_else(|| "02".into()),
        sta_ip_prefix: tc.sta.as_ref().and_then(|s| s.ip_prefix.clone()).unwrap_or_else(|| "04".into()),
        sta_timeout_secs: tc.sta.as_ref().and_then(|s| s.timeout_secs).unwrap_or(5),
        sta_max_retry: tc.sta.as_ref().and_then(|s| s.max_retry).unwrap_or(5),
        sta_retry_interval_secs: tc.sta.as_ref().and_then(|s| s.retry_interval_secs).unwrap_or(5),
        socket_rmem: tc.socket.as_ref().and_then(|s| s.rmem_max).unwrap_or(4 * 1024 * 1024),
        socket_wmem: tc.socket.as_ref().and_then(|s| s.wmem_max).unwrap_or(2 * 1024 * 1024),
        ka_echo_interval_secs: tc.keepalive.as_ref().and_then(|k| k.echo_interval_secs).unwrap_or(30),
        ka_ka_interval_secs: tc.keepalive.as_ref().and_then(|k| k.ka_interval_secs).unwrap_or(30),
        ka_max_retry: tc.keepalive.as_ref().and_then(|k| k.max_retry).unwrap_or(3),
    })
}
