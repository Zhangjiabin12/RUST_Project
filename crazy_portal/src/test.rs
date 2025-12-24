extern crate pnet;

use pnet::datalink::{self, NetworkInterface};
use pnet::packet::ethernet::{EthernetPacket, MutableEthernetPacket};
use pnet::packet::ip::{IpNextHeaderProtocol, IpNextHeaderProtocols};
use pnet::packet::ipv4::{self, MutableIpv4Packet};
use pnet::packet::tcp::{self, MutableTcpPacket, TcpFlags};
use pnet::util::checksum;

use std::net::Ipv4Addr;
use std::net::SocketAddr;

fn main() {
    // 找到网络接口
    let interfaces = datalink::interfaces();
    let interface = interfaces.iter().next().expect("No interfaces found!");

    // 构建IP数据包
    let source_ip = Ipv4Addr::new(192, 168, 0, 1);
    let dest_ip = Ipv4Addr::new(192, 168, 0, 2);
    let mut ip_packet = MutableIpv4Packet::owned(vec![0; ipv4::MutableIpv4Packet::minimum_packet_size()]).unwrap();
    ip_packet.set_version(4);
    ip_packet.set_header_length(5);
    ip_packet.set_total_length(ipv4::MutableIpv4Packet::minimum_packet_size() as u16);
    ip_packet.set_ttl(64);
    ip_packet.set_next_level_protocol(IpNextHeaderProtocols::Tcp);
    ip_packet.set_source(source_ip);
    ip_packet.set_destination(dest_ip);

    // 计算IP校验和
    let checksum = checksum(&ip_packet.to_immutable());
    ip_packet.set_checksum(checksum);

    // 构建TCP数据包
    let mut tcp_packet = MutableTcpPacket::owned(vec![0; tcp::MutableTcpPacket::minimum_packet_size()]).unwrap();
    tcp_packet.set_source(12345); // 本地端口
    tcp_packet.set_destination(80); // 目标端口
    tcp_packet.set_sequence(1000);
    tcp_packet.set_acknowledgement(0);
    tcp_packet.set_flags(TcpFlags::SYN);
    tcp_packet.set_window(64240);
    tcp_packet.set_data_offset(5); // 20字节的TCP头部
    tcp_packet.set_urgent_ptr(0);

    // 计算TCP校验和
    let checksum = checksum(&tcp_packet.to_immutable());
    tcp_packet.set_checksum(checksum);

    // 构建以太网数据包
    let mut ethernet_buffer = [0u8; 42];
    let mut ethernet_packet = MutableEthernetPacket::new(&mut ethernet_buffer).unwrap();
    ethernet_packet.set_source(interface.mac.unwrap());
    ethernet_packet.set_destination([0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]); // 广播地址
    ethernet_packet.set_ethertype(0x0800); // IPv4

    // 将IP数据包放入以太网数据包的payload中
    let ip_packet_bytes = ip_packet.packet_mut();
    ethernet_packet.set_payload(ip_packet_bytes);

    // 发送数据包
    let mut sockets = interface
        .build_datagram_socket(ipv4::IpNextHeaderProtocols::Tcp)
        .unwrap();
    let socket = sockets.next().unwrap();

    let mut sent = 0;
    while sent < 10 {
        socket.send_to(ethernet_packet.packet(), None, SocketAddr::new(dest_ip.into(), 0)).expect("send_to function failed");
        sent += 1;
    }
}
