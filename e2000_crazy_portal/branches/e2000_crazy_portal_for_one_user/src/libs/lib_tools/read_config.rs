use std::{fs, net::{IpAddr, Ipv4Addr, Ipv6Addr}};
use anyhow::Result;
use toml;

#[derive(Debug, Clone)]
pub struct ConfigData {
    pub crate_iface: i8,
    pub iface_name: String,
    pub dynmaic_ip: i8,
    pub use_ipv6: bool,
    pub start_ip: IpAddr,
    pub static_mask: IpAddr,
    pub static_gw: IpAddr,
    pub portal_ip: IpAddr,
    pub portal_user_num: usize,
    pub portal_user_head: String,
    pub portal_user_tail: usize,
    pub all_password: String,
    pub port: u16,
    pub redirect_ip: IpAddr,
    pub portal_reauth: i8,
    pub sta_mac_header: String,
    pub next_user_time: u64,
}

pub fn read_user_config() -> Result<ConfigData> {
    let config_file = r"config/portal_con";
    let contents = fs::read_to_string(config_file)?;
    let value: toml::Value = toml::from_str(&contents)?;

    let use_ipv6 = value.get("GET_IP").and_then(|v| v.get("use_ipv6")).and_then(|v| v.as_bool()).unwrap_or(false);

    let (start_ip, static_mask, static_gw) = if use_ipv6 {
        (
            value.get("GET_IP").and_then(|v| v.get("start_ip_v6")).and_then(|v| v.as_str()).unwrap_or("::").parse().unwrap_or(IpAddr::V6(Ipv6Addr::new(0,0,0,0,0,0,0,0))),
            IpAddr::V6(Ipv6Addr::new(0xffff, 0xffff, 0xffff, 0xffff, 0, 0, 0, 0)), // 64
            value.get("GET_IP").and_then(|v| v.get("static_gw_v6")).and_then(|v| v.as_str()).unwrap_or("::").parse().unwrap_or(IpAddr::V6(Ipv6Addr::new(0,0,0,0,0,0,0,0)))
        )
    } else {
        (
            value.get("GET_IP").and_then(|v| v.get("start_ip_v4")).and_then(|v| v.as_str()).unwrap_or("0.0.0.0").parse().unwrap_or(IpAddr::V4(Ipv4Addr::new(0,0,0,0))),
            value.get("GET_IP").and_then(|v| v.get("static_mask_v4")).and_then(|v| v.as_str()).unwrap_or("0.0.0.0").parse().unwrap_or(IpAddr::V4(Ipv4Addr::new(0,0,0,0))),
            value.get("GET_IP").and_then(|v| v.get("static_gw_v4")).and_then(|v| v.as_str()).unwrap_or("0.0.0.0").parse().unwrap_or(IpAddr::V4(Ipv4Addr::new(0,0,0,0)))
        )
    };

    Ok(ConfigData {
        crate_iface: value.get("CRATE_IFACE").and_then(|v| v.get("crate_iface")).and_then(|v| v.as_integer()).unwrap_or(0) as i8,
        iface_name: value.get("CRATE_IFACE").and_then(|v| v.get("iface_name")).and_then(|v| v.as_str()).unwrap_or("").to_string(),
        dynmaic_ip: value.get("GET_IP").and_then(|v| v.get("dynmaic_ip")).and_then(|v| v.as_integer()).unwrap_or(1) as i8,
        use_ipv6,
        start_ip,
        static_mask,
        static_gw,
        portal_ip: value.get("PORTAL_SERVER").and_then(|v| v.get("portal_ip_or_ipv6")).and_then(|v| v.as_str()).unwrap_or("0.0.0.0").parse().unwrap_or(IpAddr::V4(Ipv4Addr::new(0,0,0,0))),
        portal_user_num: value.get("PORTAL_USER").and_then(|v| v.get("portal_user_num")).and_then(|v| v.as_integer()).unwrap_or(0) as usize,
        portal_user_head: value.get("PORTAL_USER").and_then(|v| v.get("portal_user_head")).and_then(|v| v.as_str()).unwrap_or("").to_string(),
        portal_user_tail: value.get("PORTAL_USER").and_then(|v| v.get("portal_user_tail")).and_then(|v| v.as_integer()).unwrap_or(0) as usize,
        all_password: value.get("PORTAL_USER").and_then(|v| v.get("all_password")).and_then(|v| v.as_str()).unwrap_or("").to_string(),
        port: value.get("PORTAL_SERVER").and_then(|v| v.get("portal_port")).and_then(|v| v.as_integer()).unwrap_or(80) as u16,
        redirect_ip: value.get("REDIRECT_IP").and_then(|v| v.get("redirect_ip_or_ipv6")).and_then(|v| v.as_str()).unwrap_or("0.0.0.0").parse().unwrap_or(IpAddr::V4(Ipv4Addr::new(0,0,0,0))),
        portal_reauth: value.get("ReAuth").and_then(|v| v.get("portal_reauth")).and_then(|v| v.as_integer()).unwrap_or(0) as i8,
        sta_mac_header: value.get("STA_MAC_HEADER").and_then(|v| v.get("sta_mac_header")).and_then(|v| v.as_str()).unwrap_or("").to_string(),
        next_user_time: value.get("PORTAL_USER").and_then(|v| v.get("next_user_time")).and_then(|v| v.as_integer()).unwrap_or(0) as u64,
    })
}

#[test]
fn test_read_user_config() {
    let config_data = read_user_config().unwrap();
    println!("config_data: {:?}", config_data);
}
