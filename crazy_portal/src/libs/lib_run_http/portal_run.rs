
use url::Url;
use std::sync::Arc;
use tokio::time::sleep;
use std::time::Duration;
use tokio::net::TcpStream;
use std::collections::HashMap;
use tracing::{debug, info,error };
use fast_async_mutex::mutex::Mutex;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use crate::lib_tools::user_config::UserConfig;
use crate::lib_http::http_request::HttpRequest;
use crate::lib_http::http_response::HttpResponse;
use crate::lib_http::http_response::HttpRedirectResponse;



pub async fn run_portal(tcpsocket_mutex1: &Arc<Mutex<TcpStream>>,tcpsocket_mutex2: &Arc<Mutex<TcpStream>>, index: usize, user_config: &Arc<Mutex<UserConfig>>, ) -> anyhow::Result<()> {

    let user = user_config.lock().await;
    let username = &user.username;
    let passwd = &user.password;
    
    // drop(user);

    let mut moved_stream = tcpsocket_mutex1.lock().await;

    let mut portal_stream = tcpsocket_mutex2.lock().await;
    
    let mut data_buffer = [0u8; 1500];
    
    // let addr = format!("{}:{}", addr, portal_config.port).parse().unwrap();
    
    let http_request = HttpRequest {
        method: "GET".to_string(),
        request_uri: "/".to_string(),
        http_version: "HTTP/1.1".to_string(),
        connection: "keep-alive".to_string(),
        host: "2.2.2.2".to_string(),
        user_agent: "ZJB:CRAZY_PORTAL_AGENT".to_string(),
    }.to_bytes();

    moved_stream.write_all(&http_request).await.unwrap();
    info!("STA{} Send http request to portal server.", index);


    

    loop {
        let result = moved_stream.read(&mut data_buffer).await;
        match result {
            Ok(0) => {
                info!("STA{} connection closed or cant", index);
                break;
            }
            Ok(_n) => {
                
            }
            Err(e) => {
                error!("STA{} read error: {}", index, e);
                break;
            }
            
        }
    }

    // 2
    let http_response_text = String::from_utf8_lossy(&data_buffer).to_string();
    let http_response = HttpResponse::from_raw_text(&http_response_text);
    // println!("portal_response: {:?}", http_response);

    let mut portal_url_data = HashMap::new();

    // let mut host = "100.100.1.32".to_string();
    // let mut port = 80;

    if let Some(response) = http_response {
        if let Some(location) = response.location {
            match Url::parse(&location) {
                Ok(parsed_url) => {
                    let query_pairs = parsed_url.query_pairs();

                    // let host = parsed_url.host_str().unwrap_or_default().to_string();
                    // let port = parsed_url.port_or_known_default().unwrap_or(80);  

                    // println!("Parsed URL: {}", parsed_url);
                    for (key, value) in query_pairs {
                        if key == "devicetype" {
                            if let Some(start) = value.find("(") { // 查找编码后的左括号
                                let end = value.find(")").unwrap_or(value.len()); // 查找编码后的右括号
                                let device_type = &value[..start]; // 主类型
                                let version = &value[start + 1..end]; // 版本，跳过编码的括号 %28
                                portal_url_data.insert("devicetype".to_string(), device_type.to_string());
                                portal_url_data.insert("devicetype_version".to_string(), version.to_string());
                            }else {
                                println!("Cant find device_type and version");
                            }
                        }else {
                            portal_url_data.insert(key.to_string(), value.to_string());
                        }
                        
                    }
                },
                Err(e) => error!("解析URL失败: {}", e),
            }
        } else {
            error!("没有找到Location头部");
            panic!("没有找到Location头部");
        }
    } else {
        error!("解析HTTP响应失败");
        panic!("解析HTTP响应失败");
    }

    
    info!("STA{} Get 302 Moved Temporarily from  server.", index);
    debug!("portal_url_data: {:?}", portal_url_data);
// 构建新请求的URI
    let base_uri = "/portal/Auth.do";
    let query_string = format!(
        "error_code={}&wlanuserip={}&wlanapname={}&wlanacname={}&wlanusermac={}&wlanapip={}&vlan={}&wlanacip={}&wlanapmac={}&ssid={}&nasid={}&devicetype={}({})&apsn={}&srcurl={}&timestamp={}&sign={}&osType=Linux64",
        urlencoding::encode(portal_url_data.get("error_code").unwrap_or(&"".to_string())),
        urlencoding::encode(portal_url_data.get("wlanuserip").unwrap_or(&"".to_string())),
        urlencoding::encode(portal_url_data.get("wlanapname").unwrap_or(&"".to_string())),
        urlencoding::encode(portal_url_data.get("wlanacname").unwrap_or(&"".to_string())),
        urlencoding::encode(portal_url_data.get("wlanusermac").unwrap_or(&"".to_string())),
        urlencoding::encode(portal_url_data.get("wlanapip").unwrap_or(&"".to_string())),
        urlencoding::encode(portal_url_data.get("vlan").unwrap_or(&"".to_string())),
        urlencoding::encode(portal_url_data.get("wlanacip").unwrap_or(&"".to_string())),
        urlencoding::encode(portal_url_data.get("wlanapmac").unwrap_or(&"".to_string())),
        urlencoding::encode(portal_url_data.get("ssid").unwrap_or(&"".to_string())),  // 空值也需要编码处理
        urlencoding::encode(portal_url_data.get("nasid").unwrap_or(&"".to_string())),
        urlencoding::encode(portal_url_data.get("devicetype").unwrap_or(&"".to_string())),
        urlencoding::encode(portal_url_data.get("devicetype_version").unwrap_or(&"".to_string())),
        urlencoding::encode(portal_url_data.get("apsn").unwrap_or(&"".to_string())),
        urlencoding::encode(portal_url_data.get("srcurl").unwrap_or(&"".to_string())),
        urlencoding::encode(portal_url_data.get("timestamp").unwrap_or(&"".to_string())),
        urlencoding::encode(portal_url_data.get("sign").unwrap_or(&"".to_string())),
    );

    let full_request_uri = format!("{}?{}", base_uri, query_string);

        let request2 = HttpRequest {
            method: "GET".to_string(),
            request_uri: full_request_uri,
            http_version: "HTTP/1.1".to_string(),
            connection: "keep-alive".to_string(),
            host: "100.100.1.32".to_string(),
            user_agent: "ZJB:CRAZY_PORTAL_AGENT".to_string(),
        }.to_bytes();


        portal_stream.write_all(&request2).await.unwrap();
        info!("STA{} Send doAuth to portal server.", index);
        portal_stream.read(&mut data_buffer).await.unwrap();


        let http_response_text2 = String::from_utf8_lossy(&data_buffer).to_string();
        let http_response2 = HttpRedirectResponse::from_raw_text(&http_response_text2);
        let mut portal_url_data2 = HashMap::new();

        if let Some(response) = http_response2 {
            // println!("response: {:?}", response);

            if let Some(location) = response.location {
                match Url::parse(&location) {
                    Ok(parsed_url) => {
                        let query_pairs = parsed_url.query_pairs();
                        // println!("Parsed URL: {}", parsed_url);
                        for (key, value) in query_pairs {
                            portal_url_data2.insert(key.to_string(), value.to_string());
                        }
                    },
                    Err(e) => error!("解析URL失败: {}", e),
                }
                if !response.cookie.is_empty() {
                    portal_url_data2.insert("cookie".to_string(), response.cookie.clone());
                }
        
                // println!("portal_url_data2: {:?}", portal_url_data2);
            } else {
                error!("没有找到Location头部");
            }

        }
        
         else {
            error!("解析HTTP响应失败");
            panic!("解析HTTP响应失败");
        }
        
    debug!("portal_url_data2: {:?}", portal_url_data2);

    let mut user_map = HashMap::new();

    user_map.insert("account", username);
    user_map.insert("password", passwd);

    


    let query_string2 = format!(
        "method=doAuth&siteid=1&account={}&password={}&token={}",
        urlencoding::encode(user_map.get("account").unwrap()),
        urlencoding::encode(user_map.get("password").unwrap()),
        urlencoding::encode(portal_url_data2.get("token").unwrap_or(&"".to_string())),

    );  
        
    let full_request_uri = format!("{}?{}", base_uri, query_string2);

        let request2 = HttpRequest {
            method: "GET".to_string(),
            request_uri: full_request_uri,
            http_version: "HTTP/1.1".to_string(),
            connection: "keep-alive".to_string(),
            host: "100.100.1.32".to_string(),
            user_agent: "ZJB:CRAZY_PORTAL_AGENT".to_string(),
        }.to_bytes();

        portal_stream.write_all(&request2).await.unwrap();
    
    sleep(Duration::from_secs(60*10)).await;


    let request3_url = format!("method=offline&siteid=1&token={}",urlencoding::encode(portal_url_data2.get("token").unwrap_or(&"".to_string())));
    let full_request_uri = format!("{}?{}", base_uri, request3_url);
    let request3 = HttpRequest {
        method: "GET".to_string(),
        request_uri: full_request_uri,
        http_version: "HTTP/1.1".to_string(),
        connection: "keep-alive".to_string(),
        host: "100.100.1.32".to_string(),
        user_agent: "ZJB:CRAZY_PORTAL_AGENT".to_string(),
    }.to_bytes();

    portal_stream.write_all(&request3).await.unwrap();
    Ok(())
}