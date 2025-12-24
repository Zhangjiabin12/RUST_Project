use crate::lib_tools::read_config::ConfigData;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use tracing::{debug, error, info, trace};
use std::io::Error;
use indicatif::{ProgressBar, ProgressStyle};
use std::time::Instant;
use std::process::{Output, Command, Stdio};
use std::io::ErrorKind;

// const STA_BASE_MAC_PREFIX: &str = "000172";

#[derive(Debug)]
pub struct InterfaceManager {
    parent_interface: String,
    base_name: String,
    base_mac: String,
    start_ip: IpAddr,
    mask: IpAddr,
    gw: IpAddr,
    interface_count: usize,
    // sta_mac_header: String,
}

impl InterfaceManager {
    pub fn new(user_config: &ConfigData) -> Result<i8, Error> {
        let mut iface_config = InterfaceManager {
            parent_interface: user_config.iface_name.clone(),
            base_name: format!("{}{}", user_config.iface_name, "."),
            base_mac: user_config.sta_mac_header.clone(),
            start_ip: user_config.start_ip,
            interface_count: user_config.portal_user_num,
            mask: user_config.static_mask,
            gw: user_config.static_gw,
            // sta_mac_header: user_config.sta_mac_header
        };
        let crate_iface = user_config.crate_iface;
        match crate_iface {
            1 => {
                let result = iface_config.create_interface();
                match result {
                    Ok(()) => {}
                    Err(er) => {
                        error!("create interface error: {:?}", er);
                        panic!("create interface error: {:?}", er);
                    }
                }
                return Ok(1);
            }
            0 => {
                iface_config.delete_interfaces();
                return Ok(0);
            }
            _ => {
                error!("Invalid value for crate_iface: {}", crate_iface);
                return Ok(-1);
            }
        }
    }

    fn create_interface(&mut self) -> Result<(), Error> {
        // 生成MAC地址
        info!("create interface...");
        let pb = ProgressBar::new(self.interface_count as u64);
        pb.set_style(ProgressStyle::default_bar().template("{wide_bar} {pos}/{len}").expect("error to create progress bar"));
        pb.tick();
        self.run_command(&format!("ip link set {} promisc on", self.parent_interface)).unwrap();  // 将父接口创建为混杂模式
        
        let (ip_addr_list, _network_addr) = iterate_ips(self.start_ip, self.mask, self.interface_count as u32);
        let cidr = mask_to_cidr(self.mask);
        let start_time = Instant::now(); // 记录开始时间
        for (i, ip) in ip_addr_list.iter().enumerate() {
            let i = i + 1;
            let name = format!("{}{}", self.base_name, i);
            let end = format!("{:06x}", i);
            // 创建一个接口mac地址字符串，格式为 base_mac + end 
            let interface_mac_str = format!("{}{}", self.base_mac, end);
            let interface_mac = interface_mac_str.as_str().chars().collect::<Vec<_>>();
            self.run_command(&format!("ip link add link {} name {} type macvlan mode private", self.parent_interface, name)).unwrap();
            self.run_command(&format!("ip link set dev {} address {}", name, interface_mac.chunks(2).map(|chunk| chunk.iter().collect::<String>()).collect::<Vec<String>>().join(":"))).unwrap();
            self.run_command(&format!("ip link set {} up", name)).unwrap();
            self.run_command(&format!("ip addr add {}/{} dev {}", ip, cidr, name)).unwrap();
            if self.start_ip.is_ipv4() {
                self.run_command(&format!("route add -net {}/{} gw {} dev {}", "0.0.0.0", "0", self.gw, name)).unwrap();
            } else {
                self.run_command(&format!("ip -6 route add default via {} dev {}", self.gw, name)).unwrap();
            }
            pb.inc(1);
        }
        pb.finish();
        let end_time = Instant::now(); // 记录结束时间
        let elapsed_time = end_time.duration_since(start_time); // 计算时间间隔
        debug!("{:#?}", elapsed_time);
        info!("Interfaces created successfully in {:#?}", elapsed_time);
        Ok(())
    }

    fn delete_interfaces(&self) {
        info!("Deleting interfaces:");
        let start_time = Instant::now(); // 记录开始时间
        let pb = ProgressBar::new(self.interface_count as u64);
        pb.set_style(ProgressStyle::default_bar().template("{wide_bar} {pos}/{len}").expect("error to create progress bar"));
        pb.tick();

        for i in 1..=self.interface_count {
            let interface_name = format!("{}{}", self.base_name, i);
            let result = self.run_command(&format!("ip link delete {}", interface_name));
            match result {
                Ok(_output) => {}
                Err(err) => {
                    match err.kind() {
                        ErrorKind::Other => {}
                        _ => {}
                    }
                }
            }
            pb.inc(1);
        }
        pb.finish();
        let end_time = Instant::now(); // 记录结束时间
        let elapsed_time = end_time.duration_since(start_time); // 计算时间间隔
        debug!("Interfaces deleted successfully in {:#?}", elapsed_time);
    }

    pub fn run_command(&self, command: &str) -> Result<Output, std::io::Error> {
        let _output = Command::new("sh")
            .arg("-c")
            .arg(command)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()?;
        
        if _output.status.success() {
            Ok(_output)
        } else {
            let error_message = String::from_utf8_lossy(&_output.stderr);

            if error_message.contains("RTNETLINK answers: File exists") {
                return Ok(_output);
            } else if error_message.contains("RTNETLINK answers: Operation not permitted") {
                return Err(std::io::Error::new(std::io::ErrorKind::PermissionDenied, "Check your portal_con, or Run as root"));
            } else if error_message.contains("SIOCADDRT") {
                return Ok(_output);
            } else {
                error!("{}", error_message);
                Err(std::io::Error::new(std::io::ErrorKind::Other, "Check your portal_con, or Run as root"))
            }
        }
    }
}

fn calculate_number_of_addresses(mask: IpAddr) -> u32 {
    match mask {
        IpAddr::V4(mask_v4) => {
            let bits = mask_v4.octets().iter().fold(0, |acc, &octet| acc + octet.count_ones());
            2u32.pow(32 - bits)
        }
        IpAddr::V6(_) => {
            unimplemented!("IPv6 support is not implemented yet for calculating number of addresses");
        }
    }
}

pub fn iterate_ips(start_ip_addr: IpAddr, mask: IpAddr, user_nums: u32) -> (Vec<IpAddr>, IpAddr) {
    match (start_ip_addr, mask) {
        (IpAddr::V4(start_ip_v4), IpAddr::V4(mask_v4)) => {
            // 获取起始IP地址
            let mut ip_addr_vec = Vec::new();
            let start_ip = u32::from(start_ip_v4);

            // 获取网络地址
            let network = u32::from(start_ip_v4) & u32::from(mask_v4); // 获取网络起始地址
            trace!("Network: {}", Ipv4Addr::from(network));
            let network_addr = IpAddr::V4(Ipv4Addr::from(network));

            // 获取广播地址
            let broadcast = network | (!u32::from(mask_v4)); // 获取广播地址
            trace!("Broadcast: {}", Ipv4Addr::from(broadcast));

            if start_ip < network {
                panic!("起始地址小于网络地址，请检查起始地址和子网掩码是否合理");
            }

            if user_nums + start_ip > broadcast {
                panic!("用户数量加起始地址大于广播地址，请检查用户数和地址数是否合理");
            }

            if network > broadcast {
                panic!("网络地址大于广播地址，请检查起始地址和子网掩码是否合理");
            }

            let number_of_addresses = calculate_number_of_addresses(IpAddr::V4(mask_v4));
            if user_nums > number_of_addresses {
                panic!(" 用户数量大于地址数，请检查用户数和地址数是否合理");
            }

            for ip in (start_ip + 1)..start_ip + user_nums + 1 { // 遍历从网络地址+1到广播地址-1的所有地址
                let ip_addr = IpAddr::V4(Ipv4Addr::from(ip));
                ip_addr_vec.push(ip_addr);
            }

            (ip_addr_vec, network_addr)
        }
        (IpAddr::V6(start_ip_v6), IpAddr::V6(mask_v6)) => {
            let mut ip_addr_vec = Vec::new();
            let mut current_ip = start_ip_v6;

            let prefix_len = mask_to_cidr(IpAddr::V6(mask_v6)) as u128;

            for _ in 0..user_nums {
                ip_addr_vec.push(IpAddr::V6(current_ip));
                current_ip = increment_ipv6(current_ip, prefix_len);
            }

            (ip_addr_vec, IpAddr::V6(start_ip_v6))
        }
        _ => {
            panic!("IP address and mask must be of the same type (both IPv4 or both IPv6)");
        }
    }
}

fn increment_ipv6(ip: Ipv6Addr, prefix_len: u128) -> Ipv6Addr {
    let mut segments = ip.segments();

    // Convert segments to a single 128-bit integer
    let mut ip_int: u128 = 0;
    for segment in segments.iter() {
        ip_int = (ip_int << 16) | *segment as u128;
    }

    // Increment the address by 1
    ip_int += 1;

    // Apply the prefix mask to ensure we stay within the prefix length
    let mask = !0u128 << (128 - prefix_len);
    ip_int &= mask | !mask;

    // Convert back to segments
    for i in (0..8).rev() {
        segments[i] = (ip_int & 0xFFFF) as u16;
        ip_int >>= 16;
    }

    Ipv6Addr::from(segments)
}


fn mask_to_cidr(mask: IpAddr) -> u32 {
    match mask {
        IpAddr::V4(mask_v4) => {
            mask_v4.octets()
                .iter()
                .map(|&octet| octet.count_ones())
                .sum()
        }
        IpAddr::V6(mask_v6) => {
            mask_v6.segments()
                .iter()
                .map(|&segment| segment.count_ones())
                .sum()
        }
    }
}

#[test]
fn test_calculate_number_of_addresses() {
    let mask = IpAddr::V4(Ipv4Addr::new(255, 255, 255, 0));
    assert_eq!(calculate_number_of_addresses(mask) - 2, 254);
}

#[test]
fn test_iterate_ips() {
    let start_ip_addr = IpAddr::V4(Ipv4Addr::new(192, 168, 100, 10));
    let mask = IpAddr::V4(Ipv4Addr::new(255, 255, 255, 0));
    iterate_ips(start_ip_addr, mask, 200);

    let start_ip_addr_v6 = IpAddr::V6(Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1));
    let mask_v6 = IpAddr::V6(Ipv6Addr::new(0xffff, 0xffff, 0xffff, 0xffff, 0, 0, 0, 0));
    iterate_ips(start_ip_addr_v6, mask_v6, 200);
}

#[test]
fn test_mask_to_cidr() {
    let mask = IpAddr::V4(Ipv4Addr::new(255, 255, 255, 0));
    assert_eq!(mask_to_cidr(mask), 24);

    let mask_v6 = IpAddr::V6(Ipv6Addr::new(0xffff, 0xffff, 0xffff, 0xffff, 0, 0, 0, 0));
    assert_eq!(mask_to_cidr(mask_v6), 64);
}

#[test]
fn test_increment_ipv6() {
    let ip = Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 0);
    println!("ip: {:?}", ip);
    let next_ip = increment_ipv6(ip, 64);
    println!("next_ip: {:?}", next_ip);
    assert_eq!(next_ip, Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1));
}
