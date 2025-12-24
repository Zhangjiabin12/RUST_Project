
use fast_async_mutex::mutex::Mutex;
use tokio;
use std::sync::Arc;
use std::collections::HashMap;
use tracing::{info, debug, error, trace};
use libs::lib_tools::read_config;
use libs::lib_run_http::portal_run::run_portal;
use libs::lib_tools::user_config::UserConfig;
use libs::lib_tools::config_iface::{InterfaceManager,iterate_ips};
use libs::lib_tools::tcp_socket_bind::tcp_socket_bind_device;




#[tokio::main]
async fn main() -> anyhow::Result<()> {

    // 初始化日志
    tracing_subscriber::fmt::init();

    // 读取配置
    let portal_config = match read_config::read_user_config(){
        Ok(config) => {
            debug!("Successfully read config file: ");
        config
        },
        Err(e) => {
            error!("Error: {}, Failed to read user config file", e);
            std::process::exit(1);
        }
    };

    {
        // 创建虚拟子接口：使用的macvlan 技术
        let result = InterfaceManager::new(&portal_config);
        match result {
            Ok(c) => {
                match c {
                    1 => {
                        debug!("cratee vInterface Success");
                    }
                    0 => {
                        debug!("delete vInterface Success");
                    }
                    -1 => {
                        error!("cratee vInterface Error Uknow Reason");
                        std::process::exit(1);
                    }
                    _ => {
                        error!("cratee vInterface Error Uknow Reason");
                        std::process::exit(1);
                    }
                }   
            }
            Err(_err) => {
                error!("cratee vInterface Error Uknow Reason");
                std::process::exit(1);
                // panic!("cratee Vinterface Failed Please Check Err: {}",err);
            }
        }
    } 

    // 创建用户
    let user_hash_map = UserConfig::new(&portal_config);
    trace!("user_hash_map: {:#?}", user_hash_map);

    // 创建绑定socket 初始化配置

    let interface_name = Arc::new(&portal_config.iface_name);
    // interface_name: &Arc<String>, ip_addr: &str, port: u16
    let mut socket1_hash_map = HashMap::new();
    let mut socket2_hash_map = HashMap::new();

    let (ip_addr_list, _network_addr) = iterate_ips(portal_config.start_ip, portal_config.static_mask, portal_config.portal_user_num as u32);

    for (user_index, ip) in ip_addr_list.iter().enumerate() {
        let bind_socket_moved = Arc::new(Mutex::new(tcp_socket_bind_device(&Arc::new(format!("{}.{}",interface_name,user_index+1)), *ip, portal_config.port,portal_config.redirect_ip).await.unwrap()));
        let bind_socket_portal = Arc::new(Mutex::new(tcp_socket_bind_device(&Arc::new(format!("{}.{}",interface_name,user_index+1)), *ip, portal_config.port,portal_config.portal_ip).await.unwrap()));
        socket1_hash_map.insert(user_index, bind_socket_moved);
        socket2_hash_map.insert(user_index, bind_socket_portal);
    }

    info!("start run portal");

    let mut handles = Vec::new();

    for user_index in 0..portal_config.portal_user_num {

        let tcpsocket1_mutex = socket1_hash_map.get(&user_index).expect("Stream not found").clone();
        let tcpsocket2_mutex = socket2_hash_map.get(&user_index).expect("Stream not found").clone();
        let user_config = user_hash_map.get(&user_index).expect("User not found").clone();
        let portal_run_handle = tokio::spawn(async move {
            run_portal(&tcpsocket1_mutex, &tcpsocket2_mutex,user_index,&user_config,).await
        });

        handles.push(portal_run_handle);
    }

    for handle in handles {
        let _ = handle.await.unwrap();
    }

    //开启


    println!("Hello, world!");
    Ok(())
}