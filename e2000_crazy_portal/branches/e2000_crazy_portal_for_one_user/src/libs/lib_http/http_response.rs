#[derive(Debug)]
pub struct HttpResponse {
    response_version: String,
    pub status_code: u16,
    pub status_phrase: String,
    server: String,
    connection: String,
    pub location: Option<String>, 
    content_type: String,
    file_data: Vec<u8>,
}

impl HttpResponse {
    pub fn from_raw_text(raw_text: &str) -> Option<Self> {
        let mut response_version = String::new();
        let mut status_code = 0;
        let mut status_phrase = String::new();
        let mut server = String::new();
        let mut connection = String::new();
        let mut location = None; 
        let mut content_type = String::new();
        let file_data = Vec::new();

        for line in raw_text.lines() {
            if line.starts_with("HTTP/") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 3 {
                    response_version = parts[0].to_string();
                    status_code = parts[1].parse().unwrap_or(0);
                    status_phrase = parts[2..].join(" ");
                }
            } else if line.starts_with("Server:") {
                server = line.split(':').skip(1).next().unwrap_or("").trim().to_string();
            } else if line.starts_with("Connection:") {
                connection = line.split(':').skip(1).next().unwrap_or("").trim().to_string();
            } else if line.starts_with("Location:") {
                let location_value = line.splitn(2, ':').nth(1).unwrap_or("").trim().to_string();
                if location_value.starts_with("http://") || location_value.starts_with("https://") {
                    location = Some(location_value);
                }
            } else if line.starts_with("Content-Type:") {
                content_type = line.split(':').skip(1).next().unwrap_or("").trim().to_string();
            } else if line.is_empty() {
                // Empty line indicates end of headers, start of file data
                // file_data = hex::decode("3c68746d6c3e3c2f68746d6c3e").unwrap();
            }
        }

        if !response_version.is_empty() && status_code != 0 && !status_phrase.is_empty() {
            Some(HttpResponse {
                response_version,
                status_code,
                status_phrase,
                server,
                connection,
                location,
                content_type,
                file_data,
            })
        } else {
            None
        }
    }

    fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend(format!("{} {} {}\r\n", self.response_version, self.status_code, self.status_phrase).as_bytes());
        bytes.extend(format!("Server: {}\r\n", self.server).as_bytes());
        bytes.extend(format!("Connection: {}\r\n", self.connection).as_bytes());
        if let Some(location) = &self.location {
            bytes.extend(format!("Location: {}\r\n", location).as_bytes());
        }
        bytes.extend(format!("Content-Type: {}\r\n", self.content_type).as_bytes());
        bytes.extend(format!("\r\n").as_bytes());
        bytes.extend(&self.file_data);
        bytes
    }
}



#[derive(Debug)]
pub struct HttpRedirectResponse {
    status_code: u16,
    pub location: Option<String>,
    pub cookie: String,
}

impl HttpRedirectResponse {
    pub fn from_raw_text(raw_text: &str) -> Option<Self> {
        let mut status_code = 0;
        // let mut location = String::new();
        let mut location = None; 
        let mut cookie = String::new();

        for line in raw_text.lines() {
            if line.starts_with("HTTP/") && line.contains("302") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                status_code = parts[1].parse().unwrap_or(0);
            } else if line.starts_with("Location:") {
                let location_value = line.splitn(2, ':').nth(1).unwrap_or("").trim().to_string();
                if location_value.starts_with("http://") || location_value.starts_with("https://") {
                    location = Some(location_value);
                }
                // location = line.splitn(2, ':').nth(1).unwrap_or("").trim().to_string();
            } else if line.starts_with("Set-Cookie:") {
                cookie = line.splitn(2, ':').nth(1).unwrap_or("").trim().to_string();
            }
        }

        if status_code == 302 {
            Some(HttpRedirectResponse {
                status_code,
                location,
                cookie,
            })
        } else {
            None
        }
    }

    fn to_bytes(&self) -> Vec<u8> {
        println!("Redirect Response: {:?}", format!("HTTP/1.1 302 Found\r\nLocation: {:?}\r\nSet-Cookie: {}\r\n\r\n", self.location, self.cookie));
        format!("HTTP/1.1 302 Found\r\nLocation: {:?}\r\nSet-Cookie: {}\r\n\r\n", self.location, self.cookie).as_bytes().to_vec()
        
    }
}

#[derive(Debug)]
pub struct HttpResponseOk {
    response_version: String,
    pub status_code: u16,
    pub status_phrase: String,
    pub file_data: Vec<u8>,
}

impl HttpResponseOk {
    pub fn from_raw_text(raw_text: &str) -> Option<Self> {
        let mut response_version = String::new();
        let mut status_code = 0;
        let mut status_phrase = String::new();
        let mut file_data = Vec::new();

        for line in raw_text.lines() {
            if line.starts_with("HTTP/") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 3 {
                    response_version = parts[0].to_string();
                    status_code = parts[1].parse().unwrap_or(0);
                    status_phrase = parts[2..].join(" ");
                }
            } else if line.is_empty() {

            }
        }

        if !response_version.is_empty() && status_code != 0 && !status_phrase.is_empty() {
            Some(HttpResponseOk {
                response_version,
                status_code,
                status_phrase,
                file_data,
            })
        } else {
            None
        }
    }

    fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend(format!("{} {} {}\r\n", self.response_version, self.status_code, self.status_phrase).as_bytes());
        bytes.extend(&self.file_data);
        bytes
    }
}