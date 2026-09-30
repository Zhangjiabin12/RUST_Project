//! 应用编排：全局 receiver、AP 工厂、重试逻辑

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use rand::Rng;
use tokio::sync::{mpsc, Notify};
use tokio::time::sleep;
use tracing::{debug, error, info, warn};

use crate::ap::parse_helper::ParsedMessage;
use crate::ap::runner::ApRunner;
use crate::ap::state::ApEvent;
use crate::ap::types::StaConfig;
use crate::network::raw_socket::{parse_raw_frame, parse_arp_frame, ARP_OP_REQUEST, ARP_OP_REPLY, RawSocket};

pub const MAX_RETRY: u32 = 5;

// ============================================================================
// 全局 Receiver
// ============================================================================

pub fn start_global_receiver(
    sock: Arc<RawSocket>,
    registry: Arc<Mutex<HashMap<[u8; 6], mpsc::Sender<ApEvent>>>>,
    ip_registry: Arc<Mutex<HashMap<[u8; 4], mpsc::Sender<ApEvent>>>>,
    arp_nfy: Arc<Notify>,
) {
    std::thread::spawn(move || {
        let mut buf = vec![0u8; 4096];
        loop {
            let n = match sock.recv_raw(&mut buf) {
                Ok(n) => n,
                Err(e) => {
                    if e.raw_os_error() == Some(9) {
                        info!("receiver socket closed, exiting");
                        break;
                    }
                    error!("recv error: {}", e);
                    continue;
                }
            };

            let frame = &buf[..n];

            // ── ARP 处理 ──
            if let Some(arp) = parse_arp_frame(frame) {
                match arp.oper {
                    ARP_OP_REQUEST => {
                        let reg = ip_registry.lock().unwrap();
                        if let Some(tx) = reg.get(&arp.target_ip) {
                            let ev = ApEvent::ArpRequest {
                                sender_mac: arp.sender_mac,
                                sender_ip: arp.sender_ip,
                                target_ip: arp.target_ip,
                            };
                            if let Err(mpsc::error::TrySendError::Full(_)) = tx.try_send(ev) {
                                warn!("AP {:02x?} channel full, dropping ARP request", arp.target_ip);
                            }
                        } else {
                            debug!("AP {:02x?} not registered in ip_registry, dropping ARP request (sender={:02x?})",
                                arp.target_ip, arp.sender_ip);
                        }
                    }
                    ARP_OP_REPLY => {
                        if arp.sender_ip == sock.dst_ip {
                            sock.update_dst_mac(arp.sender_mac);
                            debug!("AC MAC resolved via ARP: {:02x?}", arp.sender_mac);
                            arp_nfy.notify_waiters();
                        }
                    }
                    _ => {}
                }
                continue;
            }

            // ── CAPWAP 处理 ──
            if let Some(parsed) = parse_raw_frame(frame) {
                let event = match ParsedMessage::from_raw(parsed.payload) {
                    ParsedMessage::DiscoveryResponse => ApEvent::DiscoveryResponse,
                    ParsedMessage::JoinResponse => ApEvent::JoinResponse,
                    ParsedMessage::ConfigStatusResponse => ApEvent::ConfigStatusResponse,
                    ParsedMessage::ChangeStateResponse => ApEvent::ChangeStateResponse,
                    ParsedMessage::KeepAliveResponse => ApEvent::KeepAliveResponse,
                    ParsedMessage::ConfigurationUpdateRequest { seq_number } =>
                        ApEvent::ConfigurationUpdateRequest { seq_number },
                    ParsedMessage::Ieee80211ConfigRequest { seq_number } =>
                        ApEvent::Ieee80211ConfigRequest { seq_number },
                    ParsedMessage::StationConfigurationRequest { seq_number } =>
                        ApEvent::StationConfigurationRequest { seq_number },
                    ParsedMessage::WtpEventResponse => ApEvent::WtpEventResponse,
                    ParsedMessage::EchoResponse => ApEvent::EchoResponse,
                    _ => continue,
                };

                let reg = registry.lock().unwrap();
                if let Some(tx) = reg.get(&parsed.dst_mac) {
                    if let Err(mpsc::error::TrySendError::Full(_)) = tx.try_send(event) {
                        warn!("AP {:02x?} channel full, dropping event", parsed.dst_mac);
                    }
                } else {
                    debug!("AP {:02x?} not registered, dropping event (src_ip={:02x?})",
                        parsed.dst_mac, parsed.src_ip);
                }
            }
        }
    });
}

// ============================================================================
// AP 工厂
// ============================================================================

pub fn make_runner(
    idx: u32,
    ap_cfg: &HashMap<String, String>,
    sta_cfg: &HashMap<String, String>,
    sta_cnt: u32,
    sock: &Arc<RawSocket>,
    arp_nfy: &Arc<Notify>,
    sta_timeout: u64, sta_max_retry: u32, sta_retry_interval: u64,
    ka_echo_interval: u64, ka_ka_interval: u64, ka_max_retry: u32,
) -> ApRunner {
    let mac = ap_cfg[&format!("ap{}_mac", idx)].clone();
    let ip = ap_cfg[&format!("ap{}_ip", idx)].clone();
    let serial = ap_cfg[&format!("ap{}_serial", idx)].clone();
    let r1 = ap_cfg[&format!("ap{}_radio1_mac", idx)].clone();
    let r2 = ap_cfg[&format!("ap{}_radio2_mac", idx)].clone();
    let r3 = ap_cfg.get(&format!("ap{}_radio3_mac", idx)).cloned();

    let mut stas = Vec::with_capacity(sta_cnt as usize);
    for s in 0..sta_cnt {
        let smac = sta_cfg[&format!("ap{}_sta{}_mac", idx, s)].clone();
        let sip = sta_cfg[&format!("ap{}_sta{}_ip", idx, s)].clone();
        let radio = match s {
            0..=34 => r1.clone(),
            35..=105 => r2.clone(),
            _ => r3.clone().unwrap_or_default(),
        };
        stas.push(StaConfig { ap_index: idx, sta_index: s, mac: smac, ip: sip, ap_radio_mac: radio });
    }

    ApRunner::new(idx, mac, ip, serial, r1, r2, r3, stas, sock.clone(), arp_nfy.clone(),
        sta_timeout, sta_max_retry, sta_retry_interval,
        ka_echo_interval, ka_ka_interval, ka_max_retry)
}

// ============================================================================
// AP 重试包装
// ============================================================================

pub async fn run_with_retry(
    idx: u32,
    runner: ApRunner,
    ev_rx: mpsc::Receiver<ApEvent>,
    mac: [u8; 6],
    ip: [u8; 4],
    registry: Arc<Mutex<HashMap<[u8; 6], mpsc::Sender<ApEvent>>>>,
    ip_registry: Arc<Mutex<HashMap<[u8; 4], mpsc::Sender<ApEvent>>>>,
    arp_nfy: Arc<Notify>,
    max: u32,
    sta_timeout: u64, sta_max_retry: u32, sta_retry_interval: u64,
    ka_echo_interval: u64, ka_ka_interval: u64, ka_max_retry: u32,
) {
    let ap_mac = runner.sm.ap_mac.clone();
    let ap_ip = runner.sm.ap_ip.clone();
    let ap_serial = runner.sm.ap_serial.clone();
    let r1 = runner.sm.radio1_mac.clone();
    let r2 = runner.sm.radio2_mac.clone();
    let r3 = runner.sm.radio3_mac.clone();
    let stas = runner.stas.clone();
    let raw_sock = runner.raw_sock.clone();
    let arp_nfy = arp_nfy.clone();

    let mut runner = runner;
    let mut ev_rx = ev_rx;
    let mut attempt = 0u32;

    loop {
        match runner.run(ev_rx).await {
            Ok(()) => { info!("AP{} done", idx); return; }
            Err(e) => {
                attempt += 1;
                warn!("AP{} failed: {} (attempt {}/{})", idx, e, attempt, max);
                if attempt >= max {
                    error!("AP{} exceeded max retries ({})", idx, max);
                    return;
                }
                let delay = rand::thread_rng().gen_range(180..=240);
                sleep(Duration::from_secs(delay)).await;

                let (new_tx, new_rx) = mpsc::channel::<ApEvent>(256);
                registry.lock().unwrap().insert(mac, new_tx.clone());
                ip_registry.lock().unwrap().insert(ip, new_tx);
                ev_rx = new_rx;

                runner = ApRunner::new(
                    idx,
                    ap_mac.clone(),
                    ap_ip.clone(),
                    ap_serial.clone(),
                    r1.clone(),
                    r2.clone(),
                    r3.clone(),
                    stas.clone(),
                    raw_sock.clone(),
                    arp_nfy.clone(),
                    sta_timeout, sta_max_retry, sta_retry_interval,
                    ka_echo_interval, ka_ka_interval, ka_max_retry,
                );
            }
        }
    }
}
