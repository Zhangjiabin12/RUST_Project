//! 地址转换与通用工具

/// "cc:d8:1f:c9:80:34" → [0xcc, 0xd8, 0x1f, 0xc9, 0x80, 0x34]
pub fn parse_mac_colon(s: &str) -> [u8; 6] {
    let parts: Vec<&str> = s.split(':').collect();
    let mut mac = [0u8; 6];
    for (i, p) in parts.iter().take(6).enumerate() {
        mac[i] = u8::from_str_radix(p, 16).unwrap_or(0);
    }
    mac
}

/// "3.0.0.1" → [3, 0, 0, 1]
pub fn parse_ip_dotted(s: &str) -> [u8; 4] {
    let mut ip = [0u8; 4];
    for (i, p) in s.split('.').take(4).enumerate() {
        ip[i] = p.parse().unwrap_or(0);
    }
    ip
}

/// "02000002" → [2, 0, 0, 2]
pub fn parse_ip_hex(s: &str) -> [u8; 4] {
    [
        u8::from_str_radix(&s[0..2], 16).unwrap_or(0),
        u8::from_str_radix(&s[2..4], 16).unwrap_or(0),
        u8::from_str_radix(&s[4..6], 16).unwrap_or(0),
        u8::from_str_radix(&s[6..8], 16).unwrap_or(0),
    ]
}

/// "ccd81f600010" → [0xcc, 0xd8, 0x1f, 0x60, 0x00, 0x10]
pub fn parse_mac_hex(s: &str) -> [u8; 6] {
    let mut b = [0u8; 6];
    for i in 0..6 {
        b[i] = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).unwrap_or(0);
    }
    b
}

/// [0xcc, 0xd8, 0x1f, 0x60, 0x00, 0x10] → "ccd81f600010"
pub fn mac_bytes_to_hex(mac: &[u8; 6]) -> String {
    format!(
        "{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]
    )
}
