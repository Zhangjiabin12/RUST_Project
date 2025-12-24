use fast_async_mutex::mutex::Mutex;
use std::sync::Arc;
use crate::lib_tools::read_config::ConfigData;
use std::collections::HashMap;

#[derive(Debug)]
pub struct UserConfig {
    index: usize,
    pub username: String,
    pub password: String,
    pub host2: String
}

impl UserConfig {

    pub fn new(portal_config : &ConfigData) -> HashMap<usize, Arc<Mutex<UserConfig>>> {
        let mut user_config_hash_map = HashMap::new();
        for index in 0..portal_config.portal_user_num {
            let user_config = Arc::new(Mutex::new(UserConfig {
                index,
                username: format!("{}{}", portal_config.portal_user_head, portal_config.portal_user_tail+index),
                password: portal_config.all_password.clone(),
                host2 : format!("{}:{}", portal_config.portal_ip, portal_config.port) 
            }));
            user_config_hash_map.insert(index, user_config);

            
        }
        user_config_hash_map
    }
}

