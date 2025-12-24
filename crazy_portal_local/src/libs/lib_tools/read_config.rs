use std::{fs, net::Ipv4Addr};

#[derive(Debug)]
pub struct ConfigData {
    pub crate_iface: i8,
    pub iface_name: String,
    pub dynmaic_ip: i8,
    pub start_ip: Ipv4Addr,
    pub static_mask:Ipv4Addr,
    pub static_gw: Ipv4Addr,
    pub portal_ip: Ipv4Addr,
    pub portal_user_num: usize,
    pub portal_user_head: String,
    pub portal_user_tail: usize,
    pub all_password: String,
    pub port: u16,
    pub redirect_ip: Ipv4Addr,
    pub portal_reauth :i8,
}

pub fn read_user_config() -> anyhow::Result<ConfigData> {
    let config_file = r"config/portal_con";

    let contents = match fs::read_to_string(config_file) {
        Ok(c) => c,
        Err(e) => {
            return Err(Box::new(e).into());
        }
    };

    let mut config_data = ConfigData {
        crate_iface: 0,
        iface_name: String::new(),
        dynmaic_ip: 1,
        start_ip: Ipv4Addr::new(0,0,0,0),
        static_mask: Ipv4Addr::new(0,0,0,0),
        static_gw: Ipv4Addr::new(0,0,0,0),
        portal_ip: Ipv4Addr::new(0,0,0,0),
        portal_user_num: 0,
        portal_user_head: String::new(),
        portal_user_tail: 0,
        all_password: String::new(),
        port: 0,
        redirect_ip: Ipv4Addr::new(0,0,0,0),
        portal_reauth:0,
    };

    for line in contents.lines() {
        if line.starts_with('#') || !line.trim().contains('=') {
            continue; // Skip comments and lines without '='
        }

        if let Some((key, value)) = line.split_once('=') {
            match key.trim() {
                "crate_iface" => config_data.crate_iface = value.trim().parse().map_err(|_| format!("portal_con have Invalid value for log_enable_and_level: {}", value.trim())).unwrap_or_default(),
                "iface_name" => config_data.iface_name = value.trim().to_string(),
                "dynmaic_ip" => config_data.dynmaic_ip = value.trim().parse().map_err(|_| format!("portal_con have Invalid value for dynmaic_ip: {}", value.trim())).unwrap_or_default(),
                "start_ip" => config_data.start_ip = value.trim().parse().map_err(|_| format!("portal_con have Invalid value for static_ip: {}", value.trim())).unwrap(),
                "static_mask" => config_data.static_mask = value.trim().parse().map_err(|_| format!("portal_con have Invalid value for static_mask: {}", value.trim())).unwrap(),
                "static_gw" => config_data.static_gw = value.trim().parse().map_err(|_| format!("portal_con have Invalid value for static_gw: {}", value.trim())).unwrap(),
                "portal_ip" => config_data.portal_ip = value.trim().parse().map_err(|_| format!("portal_con have Invalid value for portal_ip: {}", value.trim())).unwrap(),
                "portal_user_num" => config_data.portal_user_num = value.trim().parse().map_err(|_| format!("portal_con have Invalid value for portal_user_num: {}", value.trim())).unwrap(),
                "portal_user_head" => config_data.portal_user_head = value.trim().to_string(),
                "portal_user_tail" => config_data.portal_user_tail = value.trim().parse().map_err(|_| format!("portal_con have Invalid value for portal_user_tail: {}", value.trim())).unwrap(),
                "all_password" => config_data.all_password = value.trim().to_string(),
                "portal_port" => config_data.port = value.trim().parse().map_err(|_| format!("portal_con have Invalid value for port: {}", value.trim())).unwrap(),
                "redirect_ip" => config_data.redirect_ip = value.trim().parse().map_err(|_| format!("portal_con have Invalid value for redirect_ip: {}", value.trim())).unwrap(),
                "portal_reauth" => config_data.portal_reauth = value.trim().parse().map_err(|_| format!("portal_con have Invalid value for log_enable_and_level: {}", value.trim())).unwrap_or_default(),
                // Add more fields similar to above based on your configuration file
                _ => {
                    return Err(Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, format!("Invalid key: {}", key))).into());
                }
            }
        }
    }
    // println!("config_data: {:.unwrap_or_default()}", config_data);
    
    Ok(config_data)
}

#[test]
fn test_read_user_config() {
    let config_data = read_user_config().unwrap();
    println!("config_data: {:?}", config_data);
}