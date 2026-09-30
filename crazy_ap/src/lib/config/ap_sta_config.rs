use std::collections::HashMap;
use tracing::warn;

/// AP 之间的 MAC 步长（每个 AP 占 4 个连续 MAC：AP + radio1 + radio2 + radio3）
const AP_MAC_STEP: u32 = 16;

#[derive(Debug, Clone)]
pub struct ApStaConfig {
    pub ap_nums: u32,
    pub sta_count: u32,
    pub ap_config: HashMap<String, String>,
    pub sta_config: HashMap<String, String>,
}

impl ApStaConfig {
    pub fn new_ap_sta_config(
        ap_nums: &u32,
        sta_count: &u32,
        ap_mac_prefix: &str,
        ap_ip_prefix: &str,
        ap_serial_prefix: &str,
        sta_mac_prefix: &str,
        sta_ip_prefix: &str,
    ) -> anyhow::Result<Self> {
        // ── 校验 MAC 前缀合法性 ──
        let validated_ap_prefix = validate_and_fix_mac_prefix(ap_mac_prefix);
        let validated_sta_prefix = validate_sta_mac_byte(sta_mac_prefix);

        let mut ap_sta_config = ApStaConfig {
            ap_nums: *ap_nums,
            sta_count: *sta_count,
            ap_config: HashMap::new(),
            sta_config: HashMap::new(),
        };
        let mut cloned_ap_sta_config = ap_sta_config.clone();
        let ap_config = Self::get_ap_config(&mut cloned_ap_sta_config, &validated_ap_prefix, ap_ip_prefix, ap_serial_prefix);
        let sta_config = Self::get_sta_config(&mut ap_sta_config, &validated_sta_prefix, sta_ip_prefix);

        let ap_config = Self {
            ap_nums: *ap_nums,
            sta_count: *sta_count,
            ap_config: ap_config.clone(),
            sta_config: sta_config.clone(),
        };
        Ok(ap_config)
    }

    fn get_ap_config(&mut self, prefix: &str, ip_prefix: &str, serial_prefix: &str) -> &HashMap<String, String> {
        // 起始偏移，6 位十六进制从 0x010000 开始
        let base_offset = 0x010000u32;

        // 序列号基值（14 位 hex → u64）
        let serial_base = u64::from_str_radix(serial_prefix, 16).unwrap_or(0);

        for ap_num in 0..self.ap_nums {
            let offset = base_offset + ap_num * AP_MAC_STEP;

            // prefix(6位) + offset(6位) = 12 位十六进制 = 6 字节 MAC
            let ap_mac = format!("{}{:06x}", prefix, offset);
            let radio1_mac = format!("{}{:06x}", prefix, offset + 1);
            let radio2_mac = format!("{}{:06x}", prefix, offset + 2);
            let radio3_mac = format!("{}{:06x}", prefix, offset + 3);

            // ip_prefix(4位) + ap_num(4位) = 8 位十六进制 = 4 字节 IP
            let ap_ip = format!("{}{:04x}", ip_prefix, 0x0002u32 + ap_num);

            // 序列号：前缀 + 递增，固定 14 位 hex = 7 字节
            let serial = format!("{:014x}", serial_base + ap_num as u64);

            self.ap_config.insert(format!("ap{}_mac", ap_num), ap_mac);
            self.ap_config.insert(format!("ap{}_ip", ap_num), ap_ip);
            self.ap_config.insert(format!("ap{}_serial", ap_num), serial);
            self.ap_config.insert(format!("ap{}_radio1_mac", ap_num), radio1_mac);
            self.ap_config.insert(format!("ap{}_radio2_mac", ap_num), radio2_mac);
            self.ap_config.insert(format!("ap{}_radio3_mac", ap_num), radio3_mac);
        }

        &self.ap_config
    }

    fn get_sta_config(&mut self, sta_mac_prefix: &str, sta_ip_prefix: &str) -> &HashMap<String, String> {
        // MAC:  prefix(2) + {:010x}(40bit) → 乘数 65536，上限 ~16M AP
        // IP:   prefix(2) + {:06x} (24bit) → 乘数 256，  上限 ~65K AP / 256 STA
        const MAC_MUL: u32 = 65536;
        const IP_MUL: u32 = 256;

        for ap_num in 0..self.ap_nums {
            let mac_ap_offset = ap_num as u32 * MAC_MUL;
            let ip_ap_offset = ap_num as u32 * IP_MUL;

            for sta_num in 0..self.sta_count {
                let sta_mac = format!("{}{:010x}", sta_mac_prefix, mac_ap_offset + sta_num);
                let sta_ip = format!("{}{:06x}", sta_ip_prefix, ip_ap_offset + sta_num);

                self.sta_config.insert(format!("ap{}_sta{}_mac", ap_num, sta_num), sta_mac);
                self.sta_config.insert(format!("ap{}_sta{}_ip", ap_num, sta_num), sta_ip);
            }
        }

        &self.sta_config
    }
}

/// 校验 MAC 前缀合法性：首字节必须 bit0=0(单播) bit1=1(本地管理)
/// 合法首字节：0x02 0x06 0x0a 0x0e 0x12 0x16 ... 0xfe
/// 不合法则自动修正并 warn
fn validate_and_fix_mac_prefix(prefix: &str) -> String {
    if prefix.len() < 6 {
        warn!("ap_mac_prefix too short ({}), need 6 hex digits, using default 'ced81f'", prefix);
        return "ced81f".to_string();
    }

    let first_byte = match u8::from_str_radix(&prefix[..2], 16) {
        Ok(b) => b,
        Err(_) => {
            warn!("ap_mac_prefix '{}' invalid hex, using default 'ced81f'", prefix);
            return "ced81f".to_string();
        }
    };

    let is_unicast = (first_byte & 0x01) == 0;
    let is_local = (first_byte & 0x02) != 0;

    if !is_unicast || !is_local {
        let fixed = (first_byte & 0xFC) | 0x02; // 清零低2位，设 bit1=1
        let fixed_prefix = format!("{:02x}{}", fixed, &prefix[2..]);
        warn!(
            "ap_mac_prefix '{}' first byte 0x{:02x} is not locally-administered unicast, auto-fixed to '{}' (0x{:02x})",
            prefix, first_byte, fixed_prefix, fixed
        );
        fixed_prefix
    } else {
        prefix.to_string()
    }
}

/// 校验 STA MAC 首字节（2 位十六进制）：bit0=0(单播) bit1=1(本地管理)
fn validate_sta_mac_byte(prefix: &str) -> String {
    if prefix.len() < 2 {
        warn!("sta_mac_prefix too short ({}), using default '02'", prefix);
        return "02".to_string();
    }

    let first_byte = match u8::from_str_radix(&prefix[..2], 16) {
        Ok(b) => b,
        Err(_) => {
            warn!("sta_mac_prefix '{}' invalid hex, using default '02'", prefix);
            return "02".to_string();
        }
    };

    let is_unicast = (first_byte & 0x01) == 0;
    let is_local = (first_byte & 0x02) != 0;

    if !is_unicast || !is_local {
        let fixed = (first_byte & 0xFC) | 0x02;
        let fixed_prefix = format!("{:02x}", fixed);
        warn!(
            "sta_mac_prefix '{}' first byte 0x{:02x} is not locally-administered unicast, auto-fixed to '{}'",
            prefix, first_byte, fixed_prefix
        );
        fixed_prefix
    } else {
        prefix[..2].to_string()
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sta_mac_ip_edge_cases() {
        let cases: Vec<(u32, u32, u32)> = vec![
            (0, 0, 0),           // AP#0 STA#0
            (0, 0, 104),         // AP#0 STA#104
            (1, 0, 0),           // AP#1 STA#0
            (10, 10, 5),         // small
            (1000, 105, 50),     // medium
            (6655, 105, 0),      // large (current max)
            (6655, 105, 104),    // large last STA
            (10000, 105, 50),    // extra large
            (50000, 105, 100),   // stress
            (65535, 255, 255),   // max theoretical
        ];

        for (ap_nums, sta_count, check_ap) in cases {
            let cfg = ApStaConfig::new_ap_sta_config(
                &ap_nums, &sta_count, "ced81f", "0200", "27003809000000", "02", "04",
            ).unwrap();

            // Check STA MACs: must be exactly 12 hex chars
            for s in 0..sta_count.min(3) {
                let key = format!("ap{}_sta{}_mac", check_ap.min(ap_nums - 1), s);
                let mac = &cfg.sta_config[&key];
                assert_eq!(mac.len(), 12, "MAC wrong len for AP{} STA{}: {} (len={})",
                    check_ap, s, mac, mac.len());
                assert!(mac.chars().all(|c| c.is_ascii_hexdigit()),
                    "MAC non-hex for AP{} STA{}: {}", check_ap, s, mac);
            }

            // Check STA IPs: must be exactly 8 hex chars
            for s in 0..sta_count.min(3) {
                let key = format!("ap{}_sta{}_ip", check_ap.min(ap_nums - 1), s);
                let ip = &cfg.sta_config[&key];
                assert_eq!(ip.len(), 8, "IP wrong len for AP{} STA{}: {} (len={})",
                    check_ap, s, ip, ip.len());
                assert!(ip.chars().all(|c| c.is_ascii_hexdigit()),
                    "IP non-hex for AP{} STA{}: {}", check_ap, s, ip);
            }

            // Check uniqueness within same AP
            if sta_count >= 2 {
                let mac0 = &cfg.sta_config[&format!("ap{}_sta0_mac", check_ap.min(ap_nums - 1))];
                let mac1 = &cfg.sta_config[&format!("ap{}_sta1_mac", check_ap.min(ap_nums - 1))];
                assert_ne!(mac0, mac1, "STA MAC collision in same AP");
            }
        }
    }

    #[test]
    fn test_sta_mac_ip_no_collision_between_aps() {
        let cfg = ApStaConfig::new_ap_sta_config(
            &10, &20, "ced81f", "0200", "27003809000000", "02", "04",
        ).unwrap();

        for a1 in 0..10 {
            for a2 in (a1+1)..10 {
                let mac1 = &cfg.sta_config[&format!("ap{}_sta0_mac", a1)];
                let mac2 = &cfg.sta_config[&format!("ap{}_sta0_mac", a2)];
                assert_ne!(mac1, mac2, "MAC collision between AP{} and AP{}", a1, a2);

                let ip1 = &cfg.sta_config[&format!("ap{}_sta0_ip", a1)];
                let ip2 = &cfg.sta_config[&format!("ap{}_sta0_ip", a2)];
                assert_ne!(ip1, ip2, "IP collision between AP{} and AP{}", a1, a2);
            }
        }
    }

    #[test]
    fn test_validate_sta_mac_byte() {
        assert_eq!(validate_sta_mac_byte("02"), "02");
        assert_eq!(validate_sta_mac_byte("ce"), "ce");
        // 不合法应自动修正
        assert_eq!(validate_sta_mac_byte("00"), "02"); // bit1=0 → 修正为 02
        assert_eq!(validate_sta_mac_byte("01"), "02"); // bit0=1 → 修正为 02
    }

    #[test]
    fn test_large_scale_no_odd_length() {
        // 大 AP 数时 IP 的 {:06x} 不能溢出到 7 位
        let cfg = ApStaConfig::new_ap_sta_config(
            &60000, &105, "ced81f", "0200", "27003809000000", "02", "04",
        ).unwrap();

        // 抽检最后几个 AP 的 IP，必须全部是 8 字符
        for ap in [0, 1, 10000, 50000, 59999] {
            let ip = &cfg.sta_config[&format!("ap{}_sta0_ip", ap)];
            assert_eq!(ip.len(), 8, "IP overflow at AP{}: {} (len={})", ap, ip, ip.len());

            let mac = &cfg.sta_config[&format!("ap{}_sta0_mac", ap)];
            assert_eq!(mac.len(), 12, "MAC overflow at AP{}: {} (len={})", ap, mac, mac.len());
        }
    }
}
//         println!("{}: {}", key, value);
//     }
//     sleep(Duration::from_secs(3600));
// }
