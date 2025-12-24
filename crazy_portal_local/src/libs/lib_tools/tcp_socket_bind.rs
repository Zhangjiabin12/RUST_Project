use std::ffi::CString;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
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


pub async fn tcp_socket_bind_device(interface_name: &Arc<String>, ip_addr: Ipv4Addr, port: u16,target_ip: Ipv4Addr) -> io::Result<TcpStream> {
    // Create a new socket
    let socket = Socket::new(Domain::IPV4, Type::STREAM, Some(Protocol::TCP))?;

    // Set the SO_BINDTODEVICE option to bind to the specified interface
    let interface_name = CString::new(interface_name.as_ref().as_str()).expect("Invalid interface name");
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
            panic!("Failed to bind to device {:?}", interface_name);
        }
    }


    let socket_address = SocketAddr::new(IpAddr::V4(ip_addr), 0);
    let sock_addr = SockAddr::from(socket_address);

    let por_sock_addr = SocketAddr::new(IpAddr::V4(target_ip), port);

    unsafe {
        let result = bind(socket_fd, sock_addr.as_ptr(), sock_addr.len());

        if result != 0 {
            panic!("Failed to bind to IP address {}, {} ",ip_addr, Colour::Red.paint("Please run as root to cratee Interface"));
        }
    }

    // let tcp_socket = TcpSocket::from_std_stream(socket.into());
    debug!("Connecting to server at address {}", por_sock_addr);
    


    let mut attempts = 0;

    loop {
        let tcp_socket = TcpSocket::from_std_stream(socket.try_clone()?.into());
        let connect_future = tcp_socket.connect(por_sock_addr);
        match timeout(Duration::from_secs(10), connect_future).await {
            Ok(Ok(stream)) => {
                info!("Successfully connected to server at address {}", por_sock_addr);
                return Ok(stream);
            },
            Ok(Err(e)) => {
                error!("Failed to connect: {}", e);
                attempts += 1;
                if attempts >= 5 {  // 设置重试次数限制
                    return Err(io::Error::new(io::ErrorKind::Other, "Maximum retry attempts reached"));
                }
                debug!("Retrying connection attempt {}", attempts);
                tokio::time::sleep(Duration::from_secs(2)).await;  // 增加延迟以避免过快重试
            },
            Err(_) => {
                error!("Connection attempt timed out");
                attempts += 1;
                if attempts >= 5 {  // 设置重试次数限制
                    return Err(io::Error::new(io::ErrorKind::TimedOut, "Maximum retry attempts reached"));
                }
                debug!("Retrying connection attempt {}", attempts);
                tokio::time::sleep(Duration::from_secs(2)).await;  // 增加延迟以避免过快重试
            }
        }
    }
}

