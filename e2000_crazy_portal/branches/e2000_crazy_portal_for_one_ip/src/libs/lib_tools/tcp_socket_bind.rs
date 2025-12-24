use std::ffi::CString;
use std::net::{IpAddr, SocketAddr};
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::time::timeout;
use tracing::{debug, info, error};
use std::os::unix::io::AsRawFd;
use std::sync::Arc;
use libc::{setsockopt, bind, SOL_SOCKET, SO_BINDTODEVICE};
use socket2::{Domain, Protocol, Socket, SockAddr, Type};
use tokio::net::TcpSocket;
use tokio::io;
use ansi_term::Colour;

pub async fn tcp_socket_bind_device(interface_name: &Arc<String>, ip_addr: IpAddr, port: u16, target_ip: IpAddr, index: usize) -> io::Result<TcpStream> {
    // 创建新socket
    loop {
        let domain = match ip_addr {
            IpAddr::V4(_) => Domain::IPV4,
            IpAddr::V6(_) => Domain::IPV6,
        };
        let socket = Socket::new(domain, Type::STREAM, Some(Protocol::TCP))?;

        // 设置SO_BINDTODEVICE选项，绑定到指定接口
        let interface_name = CString::new(interface_name.as_ref().as_str()).expect("无效的接口名");
        let socket_fd = socket.as_raw_fd();
        unsafe {
            let result = setsockopt(
                socket_fd,
                SOL_SOCKET,
                SO_BINDTODEVICE,
                interface_name.as_ptr() as *const _,
                interface_name.to_bytes_with_nul().len() as u32,
            );

            if result != 0 {
                panic!("STA {} 绑定到设备 {:?} 失败", index, interface_name);
            }
        }

        let socket_address = SocketAddr::new(ip_addr, 0);
        let sock_addr = SockAddr::from(socket_address);

        let por_sock_addr = SocketAddr::new(target_ip, port);

        unsafe {
            let result = bind(socket_fd, sock_addr.as_ptr(), sock_addr.len());

            if result != 0 {
                panic!("STA {} 绑定到IP地址 {} 失败, {} ", index, ip_addr, Colour::Red.paint("请以root身份运行以创建接口"));
            }
        }

        debug!("STA{} 正在连接服务器地址 {}", index, por_sock_addr);

        let mut attempts = 0;

        let tcp_socket = TcpSocket::from_std_stream(socket.into());
        let connect_future = tcp_socket.connect(por_sock_addr);
        match timeout(Duration::from_secs(3), connect_future).await {
            Ok(Ok(stream)) => {
                info!("STA{} 成功连接到服务器地址 {}", index, por_sock_addr);
                return Ok(stream);
            },
            Ok(Err(e)) => {
                error!("连接失败: {}", e);
                attempts += 1;
                if attempts >= 3 {
                    return Err(io::Error::new(io::ErrorKind::Other, "已达到最大重试次数"));
                }
                debug!("STA{} 正在重试第 {} 次连接", index, attempts);
                tokio::time::sleep(Duration::from_secs(2)).await;
            },
            Err(_) => {
                error!("连接超时");
                attempts += 1;
                if attempts >= 3 {
                    return Err(io::Error::new(io::ErrorKind::TimedOut, "已达到最大重试次数"));
                }
                debug!("STA{} 正在重试第 {} 次连接", index, attempts);
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
    }
}
