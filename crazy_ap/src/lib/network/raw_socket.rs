//! AF_PACKET Raw Socket — 一个 socket 服务数千 AP 的收发
//!
//! 发包：手动拼接 Eth+IP+UDP 头 + CAPWAP 载荷，sendto 发出
//! 收包：recvfrom 收取完整帧，解析后返回 (dst_mac, payload, src_port)
//!
//! 线程安全：Linux 内核允许多线程并发 sendto 同一 AF_PACKET fd

use std::io::{self, Error, ErrorKind};
use std::mem;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

// ============================================================================
// RawSocket
// ============================================================================

pub struct RawSocket {
    fd: i32,
    if_index: i32,
    /// 物理网卡的 MAC（作为默认源 MAC）
    pub src_mac: [u8; 6],
    /// AC 的 MAC（发包时的目的 MAC，启动后可能被 ARP 更新）
    pub dst_mac: [u8; 6],
    /// ARP 解析到的 AC MAC（优先于 dst_mac）
    pub resolved_dst_mac: Mutex<Option<[u8; 6]>>,
    /// AC 的 IP
    pub dst_ip: [u8; 4],
    /// 优雅关闭标志：设置后所有 send 静默返回成功，避免 EBADF 刷屏
    shutting_down: AtomicBool,
    /// 网卡名（用于退出时恢复混杂模式）
    if_name: String,
    /// 是否开启了混杂模式
    promisc_enabled: bool,
}

impl RawSocket {
    /// 创建绑定到指定网卡的 AF_PACKET raw socket
    pub fn new(if_name: &str, dst_mac: [u8; 6], dst_ip: [u8; 4],
               rmem: u32, wmem: u32) -> io::Result<Self> {
        let if_index = get_if_index(if_name)?;
        let src_mac = get_if_mac(if_name)?;

        let fd = unsafe {
            libc::socket(
                libc::AF_PACKET,
                libc::SOCK_RAW,
                (libc::ETH_P_ALL as u16).to_be() as i32,
            )
        };
        if fd < 0 {
            return Err(Error::last_os_error());
        }

        // 绑定到指定网卡
        let sockaddr = libc::sockaddr_ll {
            sll_family: libc::AF_PACKET as u16,
            sll_protocol: (libc::ETH_P_ALL as u16).to_be(),
            sll_ifindex: if_index,
            sll_hatype: 0,
            sll_pkttype: 0,
            sll_halen: 0,
            sll_addr: [0; 8],
        };

        let ret = unsafe {
            libc::bind(
                fd,
                &sockaddr as *const _ as *const libc::sockaddr,
                mem::size_of::<libc::sockaddr_ll>() as u32,
            )
        };
        if ret < 0 {
            unsafe { libc::close(fd); }
            return Err(Error::last_os_error());
        }

        // 增大缓冲区，防止大量 AP 并发时溢出
        unsafe {
            let r = rmem as libc::c_int;
            let w = wmem as libc::c_int;
            libc::setsockopt(fd, libc::SOL_SOCKET, libc::SO_RCVBUF,
                &r as *const _ as *const libc::c_void, std::mem::size_of::<libc::c_int>() as u32);
            libc::setsockopt(fd, libc::SOL_SOCKET, libc::SO_SNDBUF,
                &w as *const _ as *const libc::c_void, std::mem::size_of::<libc::c_int>() as u32);
        }

        // 开启混杂模式（部分 NIC 驱动要求 AF_PACKET 发包时开启）
        // 若不需要可注释掉；进程正常退出时会自动恢复
        let promisc_enabled = match Self::set_promisc(if_name, fd, true) {
            Ok(()) => { tracing::info!("Interface {} set to promiscuous mode", if_name); true }
            Err(e) => { tracing::warn!("Failed to set promiscuous mode: {} (packets may not send)", e); false }
        };

        Ok(Self {
            fd, if_index, src_mac, dst_mac, dst_ip,
            resolved_dst_mac: Mutex::new(None),
            shutting_down: AtomicBool::new(false),
            if_name: if_name.to_string(),
            promisc_enabled,
        })
    }

    /// 更新 ARP 解析到的 AC MAC（全局生效）
    pub fn update_dst_mac(&self, mac: [u8; 6]) {
        *self.resolved_dst_mac.lock().unwrap() = Some(mac);
    }

    /// 获取当前生效的 AC MAC（ARP 解析优先于配置值）
    fn effective_dst_mac(&self) -> [u8; 6] {
        self.resolved_dst_mac.lock().unwrap().unwrap_or(self.dst_mac)
    }

    /// 发送一个 CAPWAP 报文（线程安全，使用 ARP 解析后的 MAC）
    ///
    /// 参数：
    /// - src_mac: 该 AP 的 MAC（可不同于物理网卡 MAC）
    /// - src_ip:  该 AP 的 IP
    /// - src_port: 源端口（随机）
    /// - dst_port: 目的端口（5246 控制 / 5247 数据）
    /// - payload:  CAPWAP 报文
    pub fn send(
        &self,
        src_mac: [u8; 6],
        src_ip: [u8; 4],
        src_port: u16,
        dst_port: u16,
        payload: &[u8],
    ) -> io::Result<usize> {
        let total = 14 + 20 + 8 + payload.len();
        let mut pkt = vec![0u8; total];

        // ── 以太网头 (14B) ──
        pkt[0..6].copy_from_slice(&self.effective_dst_mac());
        pkt[6..12].copy_from_slice(&src_mac);
        pkt[12] = 0x08; pkt[13] = 0x00; // EtherType: IPv4

        // ── IP 头 (20B) ──
        let ip_off = 14;
        pkt[ip_off] = 0x45; // Ver=4, IHL=5
        let ip_total = (20 + 8 + payload.len()) as u16;
        pkt[ip_off + 2..ip_off + 4].copy_from_slice(&ip_total.to_be_bytes());
        // ID = 0, Flags+Fragment = 0
        pkt[ip_off + 6] = 0x40; // DF
        pkt[ip_off + 8] = 64;   // TTL
        pkt[ip_off + 9] = 17;   // UDP
        pkt[ip_off + 12..ip_off + 16].copy_from_slice(&src_ip);
        pkt[ip_off + 16..ip_off + 20].copy_from_slice(&self.dst_ip);
        let ip_csum = checksum(&pkt[ip_off..ip_off + 20]);
        pkt[ip_off + 10..ip_off + 12].copy_from_slice(&ip_csum.to_be_bytes());

        // ── UDP 头 (8B) ──
        let udp_off = ip_off + 20;
        pkt[udp_off..udp_off + 2].copy_from_slice(&src_port.to_be_bytes());
        pkt[udp_off + 2..udp_off + 4].copy_from_slice(&dst_port.to_be_bytes());
        let udp_len = (8 + payload.len()) as u16;
        pkt[udp_off + 4..udp_off + 6].copy_from_slice(&udp_len.to_be_bytes());
        // UDP 校验和填 0（CAPWAP 场景 AC 一般不校验）
        pkt[udp_off + 6..udp_off + 8].copy_from_slice(&[0, 0]);

        // ── 载荷 ──
        pkt[udp_off + 8..].copy_from_slice(payload);

        // ── sendto ──
        self.send_raw(&pkt, total)
    }

    /// 标记为正在关闭，之后所有 send 操作静默返回成功
    pub fn set_shutting_down(&self) {
        self.shutting_down.store(true, Ordering::SeqCst);
    }

    /// 关闭 socket（用于 Ctrl+C 时唤醒阻塞的 recvfrom）
    pub fn shutdown(&self) {
        unsafe { libc::close(self.fd); }
    }

    /// 恢复网卡混杂模式（应在 process::exit 之前调用，因为 exit 不执行 Drop）
    pub fn restore_if(&self) {
        if self.promisc_enabled {
            let sock = unsafe { libc::socket(libc::AF_INET, libc::SOCK_DGRAM, 0) };
            if sock >= 0 {
                if let Err(e) = Self::set_promisc(&self.if_name, sock, false) {
                    tracing::warn!("Failed to restore {} promiscuous mode: {}", self.if_name, e);
                } else {
                    tracing::info!("Interface {} promiscuous mode restored", self.if_name);
                }
                unsafe { libc::close(sock); }
            }
        }
    }

    /// 设置网卡混杂模式（部分 NIC 驱动要求 AF_PACKET 发包时开启）
    fn set_promisc(if_name: &str, sock: i32, enable: bool) -> io::Result<()> {
        let mut ifr: libc::ifreq = unsafe { std::mem::zeroed() };
        let c_name = std::ffi::CString::new(if_name)
            .map_err(|_| Error::new(ErrorKind::InvalidInput, "bad iface name"))?;
        let name_bytes = c_name.as_bytes_with_nul();
        for (i, &b) in name_bytes.iter().enumerate() {
            if i >= ifr.ifr_name.len() { break; }
            ifr.ifr_name[i] = b as i8;
        }

        // 获取当前 flags
        unsafe {
            if libc::ioctl(sock, libc::SIOCGIFFLAGS, &ifr) < 0 {
                return Err(Error::last_os_error());
            }
        }

        let flags = unsafe { ifr.ifr_ifru.ifru_flags };
        let new_flags = if enable {
            flags | (libc::IFF_PROMISC as i16)
        } else {
            flags & !(libc::IFF_PROMISC as i16)
        };

        if new_flags != flags {
            unsafe {
                ifr.ifr_ifru.ifru_flags = new_flags;
                if libc::ioctl(sock, libc::SIOCSIFFLAGS, &ifr) < 0 {
                    return Err(Error::last_os_error());
                }
            }
        }
        Ok(())
    }

    /// 接收一个原始帧（阻塞调用，应在 spawn_blocking 中使用）
    ///
    /// 返回 (帧长度, 目的MAC, 载荷起始偏移)
    pub fn recv_raw(&self, buf: &mut [u8]) -> io::Result<usize> {
        let n = unsafe {
            libc::recvfrom(
                self.fd,
                buf.as_mut_ptr() as *mut libc::c_void,
                buf.len(),
                0,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        if n < 0 {
            return Err(Error::last_os_error());
        }
        Ok(n as usize)
    }

    /// 发送 ARP 请求：who-has target_ip tell sender_ip
    pub fn send_arp_request(
        &self,
        sender_mac: [u8; 6],
        sender_ip: [u8; 4],
        target_ip: [u8; 4],
    ) -> io::Result<usize> {
        self.send_arp(sender_mac, sender_ip, [0xff; 6], target_ip, 1)
    }

    /// 发送 ARP 应答：sender_ip is at sender_mac
    pub fn send_arp_reply(
        &self,
        sender_mac: [u8; 6],
        sender_ip: [u8; 4],
        target_mac: [u8; 6],
        target_ip: [u8; 4],
    ) -> io::Result<usize> {
        self.send_arp(sender_mac, sender_ip, target_mac, target_ip, 2)
    }

    /// 发送 ARP 报文（通用）
    fn send_arp(
        &self,
        sender_mac: [u8; 6],
        sender_ip: [u8; 4],
        target_mac: [u8; 6],
        target_ip: [u8; 4],
        oper: u16,
    ) -> io::Result<usize> {
        let total = 14 + 28; // Eth + ARP
        let mut pkt = vec![0u8; total];

        // Eth 头
        pkt[0..6].copy_from_slice(&target_mac);
        pkt[6..12].copy_from_slice(&sender_mac);
        pkt[12] = 0x08; pkt[13] = 0x06; // EtherType: ARP

        // ARP 载荷 (28B)
        let arp = &mut pkt[14..];
        arp[0..2].copy_from_slice(&0x0001u16.to_be_bytes());   // HTYPE: Ethernet
        arp[2..4].copy_from_slice(&0x0800u16.to_be_bytes());   // PTYPE: IPv4
        arp[4] = 6;   // HLEN
        arp[5] = 4;   // PLEN
        arp[6..8].copy_from_slice(&oper.to_be_bytes());        // OPER
        arp[8..14].copy_from_slice(&sender_mac);                // SHA
        arp[14..18].copy_from_slice(&sender_ip);                // SPA
        arp[18..24].copy_from_slice(&target_mac);               // THA
        arp[24..28].copy_from_slice(&target_ip);                // TPA

        self.send_raw(&pkt, total)
    }

    /// 底层 sendto（公共）
    fn send_raw(&self, pkt: &[u8], total: usize) -> io::Result<usize> {
        // 优雅关闭：不再真正发包，避免 EBADF
        if self.shutting_down.load(Ordering::SeqCst) {
            return Ok(0);
        }
        let sockaddr = libc::sockaddr_ll {
            sll_family: libc::AF_PACKET as u16,
            sll_protocol: (libc::ETH_P_ALL as u16).to_be(),
            sll_ifindex: self.if_index,
            sll_hatype: 0,
            sll_pkttype: 0,
            sll_halen: 6,
            sll_addr: [0; 8],
        };

        let n = unsafe {
            libc::sendto(
                self.fd,
                pkt.as_ptr() as *const libc::c_void,
                total,
                0,
                &sockaddr as *const _ as *const libc::sockaddr,
                mem::size_of::<libc::sockaddr_ll>() as u32,
            )
        };
        if n < 0 {
            return Err(Error::last_os_error());
        }
        Ok(n as usize)
    }
}

impl Drop for RawSocket {
    fn drop(&mut self) {
        // 退出时恢复网卡原始状态
        if self.promisc_enabled {
            let sock = unsafe { libc::socket(libc::AF_INET, libc::SOCK_DGRAM, 0) };
            if sock >= 0 {
                if let Err(e) = Self::set_promisc(&self.if_name, sock, false) {
                    tracing::warn!("Failed to restore promiscuous mode on {}: {}", self.if_name, e);
                } else {
                    tracing::info!("Interface {} promiscuous mode restored", self.if_name);
                }
                unsafe { libc::close(sock); }
            }
        }
        unsafe { libc::close(self.fd); }
    }
}

// SAFETY: Linux AF_PACKET sockets support concurrent sendto from multiple threads
unsafe impl Send for RawSocket {}
unsafe impl Sync for RawSocket {}

// ============================================================================
// 帧解析
// ============================================================================

/// 从原始帧中提取 (目的MAC, 源IP, 目的端口, UDP载荷起始位置)
/// 返回 None 表示不是我们关心的包（非 IPv4/UDP/目标端口不匹配）
pub fn parse_raw_frame(frame: &[u8]) -> Option<ParsedFrame<'_>> {
    if frame.len() < 42 { return None; } // Eth(14) + IP(20) + UDP(8) 最小

    // Eth 头
    let eth_type = u16::from_be_bytes([frame[12], frame[13]]);
    if eth_type != 0x0800 { return None; } // 不是 IPv4

    let dst_mac: [u8; 6] = frame[0..6].try_into().unwrap();

    // IP 头
    let ip_off = 14;
    if frame[ip_off] >> 4 != 4 { return None; }  // 不是 IPv4
    let protocol = frame[ip_off + 9];
    if protocol != 17 { return None; }  // 不是 UDP

    let src_ip: [u8; 4] = frame[ip_off + 12..ip_off + 16].try_into().unwrap();
    let dst_ip: [u8; 4] = frame[ip_off + 16..ip_off + 20].try_into().unwrap();

    let ihl = (frame[ip_off] & 0x0F) as usize * 4;
    let udp_off = ip_off + ihl;

    let src_port = u16::from_be_bytes([frame[udp_off], frame[udp_off + 1]]);
    let dst_port = u16::from_be_bytes([frame[udp_off + 2], frame[udp_off + 3]]);

    // CAPWAP: AC 端口固定 5246(控制)/5247(数据)，AP 端口动态
    // AC→AP 时 src_port=5246/5247，AP→AC 时 dst_port=5246/5247
    let is_capwap = src_port == 5246 || src_port == 5247
                 || dst_port == 5246 || dst_port == 5247;
    if !is_capwap { return None; }

    let payload_off = udp_off + 8;
    let payload = &frame[payload_off..];

    Some(ParsedFrame {
        dst_mac,
        src_ip,
        dst_ip,
        dst_port,
        payload,
    })
}

pub struct ParsedFrame<'a> {
    pub dst_mac: [u8; 6],
    pub src_ip: [u8; 4],
    pub dst_ip: [u8; 4],
    pub dst_port: u16,
    pub payload: &'a [u8],
}

// ============================================================================
// ARP 帧解析
// ============================================================================

/// 操作类型
pub const ARP_OP_REQUEST: u16 = 1;
pub const ARP_OP_REPLY: u16 = 2;

/// 解析后的 ARP 帧
#[derive(Debug, Clone)]
pub struct ParsedArpFrame {
    pub oper: u16,
    pub sender_mac: [u8; 6],
    pub sender_ip: [u8; 4],
    pub target_mac: [u8; 6],
    pub target_ip: [u8; 4],
}

/// 解析 ARP 帧，不是 ARP 返回 None
pub fn parse_arp_frame(frame: &[u8]) -> Option<ParsedArpFrame> {
    if frame.len() < 42 { return None; } // Eth(14) + ARP(28)

    let eth_type = u16::from_be_bytes([frame[12], frame[13]]);
    if eth_type != 0x0806 { return None; } // 不是 ARP

    let arp = &frame[14..];
    let htype = u16::from_be_bytes([arp[0], arp[1]]);
    let ptype = u16::from_be_bytes([arp[2], arp[3]]);
    if htype != 1 || ptype != 0x0800 { return None; } // 非 Eth/IPv4

    let oper = u16::from_be_bytes([arp[6], arp[7]]);
    let sender_mac: [u8; 6] = arp[8..14].try_into().unwrap();
    let sender_ip: [u8; 4] = arp[14..18].try_into().unwrap();
    let target_mac: [u8; 6] = arp[18..24].try_into().unwrap();
    let target_ip: [u8; 4] = arp[24..28].try_into().unwrap();

    Some(ParsedArpFrame { oper, sender_mac, sender_ip, target_mac, target_ip })
}

// ============================================================================
// 工具函数
// ============================================================================

fn get_if_index(if_name: &str) -> io::Result<i32> {
    let c_name = std::ffi::CString::new(if_name).map_err(|_| Error::new(ErrorKind::InvalidInput, "bad iface name"))?;
    let idx = unsafe { libc::if_nametoindex(c_name.as_ptr()) };
    if idx == 0 { Err(Error::last_os_error()) } else { Ok(idx as i32) }
}

fn get_if_mac(if_name: &str) -> io::Result<[u8; 6]> {
    // 通过 SIOCGIFHWADDR 获取 MAC
    let sock = unsafe { libc::socket(libc::AF_INET, libc::SOCK_DGRAM, 0) };
    if sock < 0 { return Err(Error::last_os_error()); }

    let mut ifr: libc::ifreq = unsafe { mem::zeroed() };
    let c_name = std::ffi::CString::new(if_name).map_err(|_| Error::new(ErrorKind::InvalidInput, "bad iface name"))?;
    let name_bytes = c_name.as_bytes_with_nul();
    let copy_len = name_bytes.len().min(ifr.ifr_name.len());
    // ifr_name 是 [i8]，需要逐个字节拷贝
    for (i, &b) in name_bytes[..copy_len].iter().enumerate() {
        ifr.ifr_name[i] = b as i8;
    }

    let ret = unsafe { libc::ioctl(sock, libc::SIOCGIFHWADDR, &ifr) };
    unsafe { libc::close(sock); }
    if ret < 0 { return Err(Error::last_os_error()); }

    let mut mac = [0u8; 6];
    // sa_data 是 [i8]，逐个转 u8（ifr_ifru 是 union，需 unsafe）
    unsafe {
        for i in 0..6 {
            mac[i] = ifr.ifr_ifru.ifru_hwaddr.sa_data[i] as u8;
        }
    }
    Ok(mac)
}

/// IPv4 头校验和
fn checksum(data: &[u8]) -> u16 {
    let mut sum = 0u32;
    for chunk in data.chunks(2) {
        let word = if chunk.len() == 2 {
            u16::from_be_bytes([chunk[0], chunk[1]]) as u32
        } else {
            (chunk[0] as u32) << 8
        };
        sum = sum.wrapping_add(word);
    }
    while sum >> 16 != 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }
    !(sum as u16)
}
