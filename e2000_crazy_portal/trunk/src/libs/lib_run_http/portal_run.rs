#[warn(deprecated)]
use url::Url;
use std::sync::Arc;
use tokio::time::sleep;
use std::time::Duration;
use tokio::net::TcpStream;
use std::collections::HashMap;
use tracing::{debug, info,error, trace};
use fast_async_mutex::mutex::Mutex;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use crate::lib_http::http_response::{HttpRedirectResponse, HttpResponseOk};
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
    let user_info = format!("{} {}", username, passwd);
    let offline_flags = &user.offline;
    let offline_time = &user.offline_time;
    // let user_encoded = encode(&user_info);

    info!("STA {} user_info: {}", index, user_info);


    let host2 = &user.host2;
    
    // HTTP 1.1 GET
    let http_request = HttpRequest {
        method: "GET".to_string(),
        request_uri: "/".to_string(),
        http_version: "HTTP/1.1".to_string(),
        connection: "keep-alive".to_string(),
        host: host2.to_string(),
        user_agent: "ZJB:CRAZY_PORTAL_AGENT".to_string(),
        // Accept: "*/*\r\n".to_string(),
        
    }.to_bytes();

    moved_stream.write_all(&http_request).await.unwrap();
    info!("STA{} Send http request to http_redirect.", index);

    loop {
        let loop_flags = 0 ;
        data_buffer.fill(0);
        let result = moved_stream.read(&mut data_buffer).await;
        match result {
            Ok(0) => {
                let loop_flags = loop_flags + 1;
                info!("STA{} connection closed or cant", index);
                moved_stream.write_all(&http_request).await.unwrap();
                info!("STA{} Send http request to http_redirect.", index);
                if loop_flags >=3 {
                    break;
                }
                
            }
            Ok(_n) => {
                debug!("STA{} Read {} bytes", index, _n);
                trace!("STA{} Read {} bytes : {:?}",index, _n, &data_buffer);
                break;
            }
            Err(e) => {
                error!("STA{} read error: {}", index, e);
                panic!();
            }
            
        }
    }

    // 关闭http_redirect tcp
    moved_stream.shutdown().await?;
    drop(moved_stream);
    // 获取URL
    let http_response_text = String::from_utf8_lossy(&data_buffer).to_string();
    let http_response = HttpResponse::from_raw_text(&http_response_text);

    let mut portal_url_data = HashMap::new();

    if let Some(response) = http_response {

        let status_code = response.status_code;
            match status_code {
                302 => {
                        let status_phrase = response.status_phrase;
                            match &status_phrase[..] {
                                "Moved Temporarily" => {
                                    info!("Get 302 Moved Temporarily");
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
                                    }else {
                                        error!("STA {} 没有找到Location头部", index);
                                        portal_stream.shutdown().await?;
                                        info!("STA {} shutdown tcp \n \n", index);
                                        sleep(Duration::from_millis(3000)).await;
                                        panic!("STA {} 没有找到Location头部", index);
                                    }
                                }
                                m => {
                                    error!("Get HTTP STATUS PHRASE ERROR {}",m);
                                    portal_stream.shutdown().await?;
                                    info!("STA {} shutdown tcp \n", index);
                                    sleep(Duration::from_millis(3000)).await;
                                    panic!();
                                }
                            }
                    }
                n => {
                            error!("HTTP STATUS ERROR {}",n);
                            portal_stream.shutdown().await?;
                            info!("STA {} shutdown tcp \n", index);
                            sleep(Duration::from_millis(3000)).await;
                            panic!()
                            }
                }
        }
        else {
            portal_stream.shutdown().await?;
            info!("STA {} shutdown tcp \n", index);
            sleep(Duration::from_millis(3000)).await;
            panic!()
        }

    trace!("portal_url_data: {:?}", portal_url_data);

    // 构建新请求的URI 第一次Auth.do
    let base_uri = "/portal/Auth.do";
    let query_string = format!(
        "wlanuserip={}&wlanuseripv6={}&wlanusermac={}&wlanacname={}&ssid={}&srcurl={}&osType=Linux64",
        urlencoding::encode(portal_url_data.get("wlanuserip").unwrap_or(&"".to_string())),
        urlencoding::encode(portal_url_data.get("wlanuseripv6").unwrap_or(&"".to_string())),
        urlencoding::encode(portal_url_data.get("wlanusermac").unwrap_or(&"".to_string())),
        urlencoding::encode(portal_url_data.get("wlanacname").unwrap_or(&"".to_string())),
        urlencoding::encode(portal_url_data.get("ssid").unwrap_or(&"".to_string())),
        urlencoding::encode(portal_url_data.get("srcurl").unwrap_or(&"".to_string())),
        );

    let full_request_uri = format!("{}?{}", base_uri, query_string);
    trace!("full_request_uri: {}", full_request_uri);

    // 根据获取的URL创建第二个Auth.do method=doAuth
    let request2 = HttpRequest {
        method: "GET".to_string(),
        request_uri: full_request_uri.clone(),
        http_version: "HTTP/1.1".to_string(),
        connection: "keep-alive".to_string(),
        host: "100.100.1.32".to_string(),
        user_agent: "ZJB:CRAZY_PORTAL_AGENT".to_string(),
        // Accept: "".to_string(),
    }.to_bytes();


    portal_stream.write_all(&request2).await.unwrap();
    // info!("STA{} Send doAuth to portal server.", index);
    loop {
        let loop_flags = 0 ;
        data_buffer.fill(0);
        let result = portal_stream.read(&mut data_buffer).await;
        match result {
            Ok(0) => {
                let loop_flags = loop_flags + 1;
                info!("STA{} connection closed or cant", index);
                portal_stream.write_all(&request2).await.unwrap();
                info!("STA{} Send doAuth to portal server.", index);
                if loop_flags >=3 {
                    break;
                }
            }
            Ok(_n) => {
                debug!("STA{} Read{} bytes", index, _n);
                trace!("STA{} Read{} bytes : {:?}",index, _n, &data_buffer);
                break;
            }
            Err(e) => {
                error!("STA{} read error: {}", index, e);
                panic!();
            }
            
        }
    }

    // 接收第二次的回复，应该是302 Found 代理token和cookie
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
                    info!("STA {} get coockie",index);
                    portal_url_data2.insert("cookie".to_string(), response.cookie.clone());
                }
        
            } else {
                error!("没有找到Location头部");
            }

        }
        
         else {
            error!("解析HTTP响应失败");
            panic!("解析HTTP响应失败");
        }
        
    trace!("portal_url_data2: {:?}", portal_url_data2);


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
    
        loop {
            data_buffer.fill(0);
            let result = portal_stream.read(&mut data_buffer).await;
            match result {
                Ok(0) => {
                    info!("STA{} connection closed or cant", index);
                    break;
                }
                Ok(_n) => {
                    info!("STA{} Get Some Response, Read {} bytes", index, _n);
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
                    if response.status_phrase == "OK" {
                        info!("STA{} Get 200 OK", index);
                        match response.code.as_str() {
                            "0" => {
                                if response.message == "NO_ERROR" {
                                    info!("STA{} is online", index);
                                } else {
                                    let err_msg = format!("Get 0 But ERROR {}", response.message);
                                    error!("{}", err_msg);
                                    panic!("{}", err_msg);
                                }
                            },
                            _ => {
                                let err_msg = format!("STA {} code :{} not online", index, response.code);
                                error!("{}", err_msg);
                                panic!("{}", err_msg);
                            }
                        }
                    } else {
                        let err_msg = format!("Get HTTP STATUS PHRASE ERROR {}", response.status_phrase);
                        error!("{}", err_msg);
                        panic!("{}", err_msg);
                    }
                },
                _ => {
                    let err_msg = format!("HTTP STATUS ERROR {}", status_code);
                    error!("{}", err_msg);
                    panic!("{}", err_msg);
                }
            }
        } else {
            panic!("No HTTP response");
        }
        
    
    if *offline_flags {
        sleep(Duration::from_secs(*offline_time)).await;
        let request3_url = format!("method=offline&siteid=1&token={}",urlencoding::encode(portal_url_data2.get("token").unwrap_or(&"".to_string())));
        let full_request_uri = format!("{}?{}", base_uri, request3_url);
        let request3 = HttpRequest {
            method: "GET".to_string(),
            request_uri: full_request_uri,
            http_version: "HTTP/1.1".to_string(),
            connection: "keep-alive".to_string(),
            host: host2.to_string(),
            user_agent: "ZJB:CRAZY_PORTAL_AGENT".to_string(),
        }.to_bytes();

        portal_stream.write_all(&request3).await.unwrap();
        info!("STA {} do offline", index);
        sleep(Duration::from_millis(200)).await;
        portal_stream.shutdown().await?;
        drop(portal_stream);

    }else {
        portal_stream.shutdown().await?;
        drop(portal_stream);
    }


   Ok(())
}

// use rand::Rng;
// use std::time::SystemTime;
// use std::time::UNIX_EPOCH;

// fn generate_unique_id() -> String {
//     // 获取自UNIX EPOCH以来的时间，以毫秒为单位
//     let start = SystemTime::now();
//     let since_the_epoch = start.duration_since(UNIX_EPOCH)
//         .expect("Time went backwards")
//         .as_millis();  // 将时间转换为毫秒
//     // println!("{}",since_the_epoch);
//     // 生成一个随机浮点数，并转换为字符串
//     let mut rng = rand::thread_rng();
//     let random_value: f64 = rng.gen();  // 生成一个0到1之间的随机浮点数
//     // let random_string = random_value.to_string().replace("0.", "");  // 转换为字符串并移除"0."

//     // 将时间戳和随机数字符串拼接
//     format!("{}{}1", since_the_epoch, random_value)
// }