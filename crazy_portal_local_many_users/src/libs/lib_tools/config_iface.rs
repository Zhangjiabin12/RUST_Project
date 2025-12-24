use crate::lib_tools::read_config::ConfigData;
use std::net::Ipv4Addr;
use tracing::{debug, error, info,trace};
use std::io::Error;
use indicatif::{ProgressBar,ProgressStyle};
use std::time::Instant;
use std::process::{Output,Command,Stdio};
use std::io::ErrorKind;

const STA_BASE_MAC_PREFIX: &str = "000172";

#[derive(Debug)]
pub struct InterfaceManager {
    parent_interface: String,
    base_name: String,
    base_mac: String,
    start_ip: Ipv4Addr,
    mask: Ipv4Addr,
    gw: Ipv4Addr,
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
                let result = InterfaceManager::create_interface(&mut iface_config);
                match result {
                    Ok(())=>{}
                    Err(er) => {
                        error!("create interface error: {:?}", er);
                        panic!("create interface error: {:?}", er);
                    }
                }
                return Ok(1);
            }
            0 => {
                InterfaceManager::delete_interfaces(&mut iface_config);
                return Ok(0)
            }
            _=> {
                error!("Invalid value for crate_iface: {}", crate_iface);
                return Ok(-1);
            }
        }
    }

    fn create_interface(&mut self) -> Result<(),Error> {
        // 生成MAC地址
        info!("create interface...");
        let pb = ProgressBar::new(self.interface_count as u64);
        pb.set_style(ProgressStyle::default_bar().template("{wide_bar} {pos}/{len}").expect("erorr to crate progress bar"));
        pb.tick();
        self.run_command(&format!("ip link set {} promisc on", self.parent_interface)).unwrap();  // 将父接口创建为混杂模式
        
        let (ip_addr_list, network_addr) = iterate_ips(self.start_ip, self.mask, self.interface_count as u32);
        let cidr = mask_to_cidr(self.mask);
        let start_time = Instant::now(); // 记录开始时间
        for (i, ip) in ip_addr_list.iter().enumerate() {
            let i = i+1;
            let name = format!("{}{}", self.base_name, i);
            let end = format!("{:04x}", i);
            // 创建一个接口mac地址字符串，格式为 base_mac + end 
            let interface_mac_str = format!("00{}{}", self.base_mac, end);
            let interface_mac = interface_mac_str.as_str().chars().collect::<Vec<_>>();
            self.run_command(&format!("ip link add link {} name {} type macvlan mode private", self.parent_interface, name)).unwrap();
            self.run_command(&format!("ip link set dev {} address {}", name, interface_mac.chunks(2).map(|chunk| chunk.iter().collect::<String>()).collect::<Vec<String>>().join(":"))).unwrap();
            self.run_command(&format!("ip link set {} up", name)).unwrap();
            self.run_command(&format!("ip addr add {}/{} dev {}", ip, cidr, name)).unwrap();
            self.run_command(&format!("route add -net {}/{} gw {} dev {}", "0.0.0.0", "0", self.gw, name, )).unwrap();
            pb.inc(1);
        }
        pb.finish();
        let end_time = Instant::now(); // 记录结束时间
        let elapsed_time = end_time.duration_since(start_time); // 计算时间间隔\
        debug!("{:#?}",elapsed_time);
        info!("Interfaces crateed successfully with {:#?}", elapsed_time);
        Ok(())
    }

    fn delete_interfaces(&self) {
        info!("Deleting interfaces:");
        let start_time = Instant::now(); // 记录开始时间
        let pb = ProgressBar::new(self.interface_count as u64);
        pb.set_style(ProgressStyle::default_bar().template("{wide_bar} {pos}/{len}").expect("erorr to crate progress bar"));
        pb.tick();

        for i in 1..=self.interface_count {
            let interface_name = format!("{}{}", self.base_name, i);
            let result = self.run_command(&format!("ip link delete {}", interface_name));
            match result {
                Ok(_output) => {}
                Err(err) => {
                    match err.kind() {
                        ErrorKind::Other => {

                        } 
                        _ => {}
                    }
                }
            }
            // println!("Deleted interface: {}", interface_name);
            pb.inc(1);
        }
        pb.finish();
        let end_time = Instant::now(); // 记录结束时间
        let elapsed_time = end_time.duration_since(start_time); // 计算时间间隔\
        debug!("Interfaces deleted successfully with {:#?}",elapsed_time);
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
            }
            else if error_message.contains("RTNETLINK answers: Operation not permitted") {
                return Err(std::io::Error::new(std::io::ErrorKind::PermissionDenied, "Check your portal_con, or Run as root"));
            }
            else if error_message.contains("SIOCADDRT") {
                return Ok(_output);
            }
            else {
                error!("{}",error_message);
                Err(std::io::Error::new(std::io::ErrorKind::Other, "Check your portal_con, or Run as root"))
            }
            
        }
    }

}


fn calculate_number_of_addresses(mask: Ipv4Addr) -> u32 {
    let bits = mask.octets().iter().fold(0, |acc, &octet| acc + octet.count_ones());
    
    2u32.pow(32 - bits)
}

pub fn iterate_ips(start_ip_addr: Ipv4Addr, mask: Ipv4Addr, user_nums: u32) -> (Vec<Ipv4Addr>, Ipv4Addr){
    // 获取起始IP地址
    let mut ip_addr_vec = Vec::new();
    let start_ip = u32::from(start_ip_addr);

    // 获取网络地址
    let network = u32::from(start_ip_addr) & u32::from(mask); // 获取网络起始地址
    trace!("Network: {}", Ipv4Addr::from(network));
    let network_addr = Ipv4Addr::from(network);

    // 获取广播地址
    let broadcast = network | (!u32::from(mask)); // 获取广播地址
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

    let number_of_addresses = calculate_number_of_addresses(mask);
    if user_nums > number_of_addresses {
        panic!(" 用户数量大于地址数，请检查用户数和地址数是否合理");
    }


    for ip in (start_ip + 1)..start_ip+user_nums+1 { // 遍历从网络地址+1到广播地址-1的所有地址
        let ip_addr = Ipv4Addr::from(ip);
        ip_addr_vec.push(ip_addr);

        // println!("{}", ip_addr);
    }

    (ip_addr_vec,network_addr)
}

fn mask_to_cidr(mask: Ipv4Addr) -> u32 {
    mask.octets()
        .iter()
        .map(|&octet| octet.count_ones())
        .sum()
}

#[test]

fn test_calculate_number_of_addresses() {
    let mask = Ipv4Addr::new(255, 255, 255, 0);
    assert_eq!(calculate_number_of_addresses(mask)-2, 254);
}

#[test]
fn test_iterate_ips() {
    let start_ip_addr = Ipv4Addr::new(192, 168, 100, 10);
    let mask = Ipv4Addr::new(255, 255, 255, 0);
    iterate_ips(start_ip_addr, mask,200);
}

#[test]
fn test_mask_to_cidr() {
    let mask = Ipv4Addr::new(255, 255, 255, 0);
    assert_eq!(mask_to_cidr(mask), 24);
}