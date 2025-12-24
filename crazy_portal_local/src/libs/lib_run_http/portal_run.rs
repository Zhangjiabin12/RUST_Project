
use url::Url;
use serde_json::json;
use std::sync::Arc;
use tokio::time::sleep;
use std::time::Duration;
use tokio::net::TcpStream;
use std::collections::HashMap;
use tracing::{debug, info,error };
use fast_async_mutex::mutex::Mutex;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use crate::lib_http::http_request::HttpRequestPost;
use crate::lib_http::http_response::HttpResponseOk;
use crate::lib_tools::user_config::UserConfig;
use crate::lib_http::http_request::HttpRequest;
use crate::lib_http::http_response::HttpResponse;



pub async fn run_portal(tcpsocket_mutex1: &Arc<Mutex<TcpStream>>,tcpsocket_mutex2: &Arc<Mutex<TcpStream>>, index: usize, user_config: &Arc<Mutex<UserConfig>>, ) -> anyhow::Result<()> {
    
    let user = user_config.lock().await;
    let mut moved_stream = tcpsocket_mutex1.lock().await;

    let mut portal_stream = tcpsocket_mutex2.lock().await;
    let mut data_buffer = [0u8; 1500];

    let username = &user.username;
    let passwd = &user.password;
    let host2 = &user.host2;
    
    let http_request = HttpRequest {
        method: "GET".to_string(),
        request_uri: "/".to_string(),
        http_version: "HTTP/1.1".to_string(),
        // connection: "keep-alive".to_string(),
        host: "1.2.3.4".to_string(),
        Accept: "*/*\r\n".to_string(),
        // user_agent: "ZJB:CRAZY_PORTAL_AGENT".to_string(),
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

    let mut portal_url_data = HashMap::new();

    // let mut host = "100.100.1.32".to_string();
    // let mut port = 80;

    if let Some(response) = http_response {
        if let Some(location) = response.location {
            match Url::parse(&location) {
                Ok(parsed_url) => {
                    let query_pairs = parsed_url.query_pairs();

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

    
    // info!("STA{} Get 302 Moved Temporarily from  server.", index);
    debug!("portal_url_data: {:?}", portal_url_data);
// 构建新请求的URI
    let base_uri = "/login.html";
    let query_string = format!(
        "error_code={}&wlanuserip={}&wlanapname={}&wlanacname={}&wlanusermac={}&wlanapip={}&vlan={}&wlanacip={}&wlanapmac={}&ssid={}&nasid={}&devicetype={}({})&apsn==115200",
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
        // urlencoding::encode(portal_url_data.get("apsn").unwrap_or(&"".to_string())),
        // urlencoding::encode(portal_url_data.get("srcurl").unwrap_or(&"".to_string())),
        // urlencoding::encode(portal_url_data.get("timestamp").unwrap_or(&"".to_string())),
        // urlencoding::encode(portal_url_data.get("sign").unwrap_or(&"".to_string())),
    );

    let full_request_uri = format!("{}?{}", base_uri, query_string);

    let http_response_text2 = String::from_utf8_lossy(&data_buffer).to_string();
    let http_response2 = HttpResponseOk::from_raw_text(&http_response_text2);
    // let mut portal_url_data2 = HashMap::new();
    
    if let Some(response) = http_response2 {

        let status_code = response.status_code;
            match status_code {
                302 => {
                        let status_phrase = response.status_phrase;
                            match &status_phrase[..] {
                                "Moved Temporarily" => {
                                    info!("Get 302 Moved Temporarily");
                                }
                                m => {
                                    error!("Get HTTP STATUS PHRASE ERROR {}",m);
                                    panic!();
                                }
                            }
                    }
                n => {
                            error!("HTTP STATUS ERROR {}",n);
                            panic!()
                            }
                }
        }
        else {
            panic!()
        }


    let unique_id = generate_unique_id();

    let http_post_login = HttpRequestPost {
        method: "POST".to_string(),
        request_uri: "/webapi/wireless/v1".to_string(),
        http_version: "HTTP/1.1".to_string(),
        connection: "keep-alive".to_string(),
        host: user.host2.to_string(),
        user_agent: "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/109.0.0.0 Safari/537.36".to_string(),
        content_type: "application/json;charset=UTF-8".to_string(),
        accept: "application/json, text/plain, */*".to_string(),
        content_length: 0, 
        x_requested_with: "XMLHttpRequest".to_string(),
        mp_expect_language: "cn".to_string(),
        origin: format!("http://{}", host2).to_string(),
        referer: format!("http://{}{}",host2,full_request_uri).to_string(),
        accept_encoding: "gzip, deflate".to_string(),
        accept_language: "zh-CN,zh;q=0.9".to_string(),
        cookie: "sessionid=0".to_string(),
        body: json!({
            "jsonrpc": "2.0",
            "method": "portallogin",
            "id": unique_id,
            "params": {
                "auth": "empiMSBhMTIzNDU2",
                "urls": {
                    "error_code": "0",
                    "wlanuserip": portal_url_data.get("wlanuserip").unwrap_or(&"".to_string()),
                    "wlanapname": portal_url_data.get("wlanapname").unwrap_or(&"".to_string()),
                    "wlanacname": portal_url_data.get("wlanacname").unwrap_or(&"".to_string()),
                    "wlanusermac": portal_url_data.get("wlanusermac").unwrap_or(&"".to_string()),
                    "wlanapip": portal_url_data.get("wlanapip").unwrap_or(&"".to_string()),
                    "vlan": "0",
                    "wlanacip": portal_url_data.get("wlanacip").unwrap_or(&"".to_string()),
                    "wlanapmac": portal_url_data.get("wlanapmac").unwrap_or(&"".to_string()),
                    "devicetype": format!("{}({})",portal_url_data.get("devicetype").unwrap_or(&"".to_string()), portal_url_data.get("devicetype_version").unwrap_or(&"".to_string()) ),
                    "apsn": "=115200"
                }
            }
        }),
    };

    let request_bytes = http_post_login.to_bytes();
    portal_stream.write_all(&request_bytes).await.unwrap();
    info!("STA{} Send portallogin",index);

    loop {
        data_buffer.fill(0);
        let result = portal_stream.read(&mut data_buffer).await;
        match result {
            Ok(0) => {
                info!("STA{} connection closed or cant", index);
                break;
            }
            Ok(_n) => {
                info!("STA{} Get Some Response", index);
                break;
            }
            Err(e) => {
                error!("STA{} read error: {}", index, e);
                break;
            }
            
        }
    }

    let http_response_text2 = String::from_utf8_lossy(&data_buffer).to_string();
    // println!("{}", http_response_text2);
    let http_response2 = HttpResponseOk::from_raw_text(&http_response_text2);
    // let mut portal_url_data2 = HashMap::new();
    
    if let Some(response) = http_response2 {

        let status_code = response.status_code;
            match status_code {
                200 => {
                        let status_phrase = response.status_phrase;
                            match &status_phrase[..] {
                                "OK" => {
                                    info!("Get 200 OK STA{} is online", index);
                                }
                                m => {
                                    error!("Get HTTP STATUS PHRASE ERROR {}",m);
                                    panic!();
                                }
                            }
                    }
                n => {
                            error!("HTTP STATUS ERROR {}",n);
                            panic!()
                            }
                }
        }
        else {
            panic!()
        }

    
    sleep(Duration::from_secs(5)).await;


    Ok(())
}

use rand::Rng;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

fn generate_unique_id() -> String {
    // 获取自UNIX EPOCH以来的时间，以毫秒为单位
    let start = SystemTime::now();
    let since_the_epoch = start.duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_millis();  // 将时间转换为毫秒
    // println!("{}",since_the_epoch);
    // 生成一个随机浮点数，并转换为字符串
    let mut rng = rand::thread_rng();
    let random_value: f64 = rng.gen();  // 生成一个0到1之间的随机浮点数
    // let random_string = random_value.to_string().replace("0.", "");  // 转换为字符串并移除"0."

    // 将时间戳和随机数字符串拼接
    format!("{}{}1", since_the_epoch, random_value)
}