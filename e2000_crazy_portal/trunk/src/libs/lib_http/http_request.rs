use serde_json::Value;

#[derive(Debug)]
pub struct HttpRequest {
    pub method: String,
    pub request_uri: String,
    pub http_version: String,
    pub connection: String,
    pub host: String,
    pub user_agent: String,
    // pub Accept: String,
}

impl HttpRequest {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut request = String::new();
        request.push_str(&format!("{} {} {}\r\n", self.method, self.request_uri, self.http_version));
        request.push_str(&format!("Host: {}\r\n", self.host));
        request.push_str(&format!("Connection: {}\r\n", self.connection));
        request.push_str(&format!("User-Agent: {}\r\n", self.user_agent));
        // request.push_str(&format!("Accept: {}\r\n", self.Accept));
        request.push_str("\r\n"); 
        request.into_bytes()
    }
}



#[derive(Debug)]
pub struct HttpRequestPost {
    pub method: String,
    pub request_uri: String,
    pub http_version: String,
    pub connection: String,
    pub host: String,
    pub user_agent: String,
    pub content_type: String,
    pub accept: String,
    pub content_length: usize,
    pub x_requested_with: String,
    pub mp_expect_language: String,
    pub origin: String,
    pub referer: String,
    pub accept_encoding: String,
    pub accept_language: String,
    pub cookie: String,
    pub body: Value,  
}

impl HttpRequestPost {
    pub fn to_bytes(&self) -> Vec<u8> {
        let json_body = serde_json::to_string(&self.body).unwrap_or_default();
        let mut request = String::new();
        request.push_str(&format!("{} {} {}\r\n", self.method, self.request_uri, self.http_version));
        request.push_str(&format!("Host: {}\r\n", self.host));
        request.push_str(&format!("Connection: {}\r\n", self.connection));
        request.push_str(&format!("Content-Length: {}\r\n", json_body.len()));
        request.push_str(&format!("Accept: {}\r\n", self.accept));
        request.push_str(&format!("X-Requested-With: {}\r\n", self.x_requested_with));
        request.push_str(&format!("User-Agent: {}\r\n", self.user_agent));
        request.push_str(&format!("MP-Expect-Language: {}\r\n", self.mp_expect_language));
        request.push_str(&format!("Content-Type: {}\r\n", self.content_type));
        request.push_str(&format!("Origin: {}\r\n", self.origin));
        request.push_str(&format!("Referer: {}\r\n", self.referer));
        request.push_str(&format!("Accept-Encoding: {}\r\n", self.accept_encoding));
        request.push_str(&format!("Accept-Language: {}\r\n", self.accept_language));
        request.push_str(&format!("Cookie: {}\r\n", self.cookie));
        request.push_str("\r\n"); 
        request.push_str(&json_body); 

        request.into_bytes()
    }
}
