//! crazy_ap —— CAPWAP AP 模拟器（Raw Socket 版）
//!
//! 架构：
//! - 1 个 AF_PACKET raw socket 负责所有 AP 的发包
//! - 1 个 receiver task 从 raw socket 收包，按目的 MAC 分发给各 AP
//! - 每个 AP 独立状态机 + keepalive/echo/STA 子任务
//! - 无需 macvlan，支持 6000+ AP

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::{mpsc, Notify};
use tokio::time::sleep;
use tracing::{error, info};

use lib::app;
use lib::ap::state::ApEvent;
use lib::config::ap_sta_config::ApStaConfig;
use lib::infra::logging;
use lib::infra::sysctl;
use lib::infra::util;
use lib::network::raw_socket::RawSocket;

// ── OS 级 SIGINT 处理（绕过 tokio 信号驱动，避免高负载下 ctrl_c 不响应）──
static SHUTDOWN: AtomicBool = AtomicBool::new(false);

extern "C" fn sigint_handler(_: i32) {
    SHUTDOWN.store(true, Ordering::SeqCst);
}

// ── 命令行参数解析 ──
const DEFAULT_CONFIG: &str = "config/wtp_con.toml";

fn parse_args() -> String {
    let args: Vec<String> = std::env::args().collect();
    let mut i = 1;
    while i < args.len() {
        if args[i] == "-c" {
            if i + 1 < args.len() {
                return args[i + 1].clone();
            } else {
                eprintln!("Error: -c requires a file path argument");
                std::process::exit(1);
            }
        }
        i += 1;
    }
    DEFAULT_CONFIG.to_string()
}

// ============================================================================
// main
// ============================================================================

#[tokio::main]
async fn main() {
    let _guard = logging::init_logging();
    let config_path = parse_args();

    // ── 1. 读配置 ──
    let cfg = match lib::config::read_config::read_user_config(&config_path) {
        Ok(c) => c,
        Err(e) => {
            error!("Config error: {}", e);
            std::process::exit(1);
        }
    };

    // ── 2. 设置内核参数 + 创建 RawSocket ──
    sysctl::apply_sysctl(cfg.socket_rmem, cfg.socket_wmem);
    let raw_sock = Arc::new(
        RawSocket::new(
            &cfg.iface_name,
            util::parse_mac_colon(&cfg.ac_mac),
            util::parse_ip_dotted(&cfg.dest_ip),
            cfg.socket_rmem,
            cfg.socket_wmem,
        )
        .expect("Failed to create raw socket. Run as root."),
    );
    info!("Raw socket created on {}", cfg.iface_name);

    // ── 3. 注册表 + ARP 通知 ──
    let registry: Arc<Mutex<HashMap<[u8; 6], mpsc::Sender<ApEvent>>>> =
        Arc::new(Mutex::new(HashMap::new()));
    let ip_registry: Arc<Mutex<HashMap<[u8; 4], mpsc::Sender<ApEvent>>>> =
        Arc::new(Mutex::new(HashMap::new()));
    let arp_nfy = Arc::new(Notify::new());

    // ── 4. 启动全局 receiver ──
    app::start_global_receiver(
        raw_sock.clone(),
        registry.clone(),
        ip_registry.clone(),
        arp_nfy.clone(),
    );

    // ── 5. 注册 SIGINT 处理器（提前注册，确保启动阶段也能响应 Ctrl+C）──
    unsafe {
        libc::signal(libc::SIGINT, sigint_handler as *const () as libc::sighandler_t);
    }

    // ── 6. 生成 AP/STA 配置并批量启动 ──
    let asc = ApStaConfig::new_ap_sta_config(
        &cfg.ap_nums,
        &cfg.sta_count,
        &cfg.ap_mac_prefix,
        &cfg.ap_ip_prefix,
        &cfg.ap_serial_prefix,
        &cfg.sta_mac_prefix,
        &cfg.sta_ip_prefix,
    )
    .expect("AP/STA config failed");

    let ap_map = &asc.ap_config;
    let sta_map = &asc.sta_config;

    info!(
        "Launching {} APs x {} STAs, batch={}, interval={}s (raw socket mode)",
        asc.ap_nums, asc.sta_count, cfg.ap_batch_size, cfg.ap_thread_interval
    );

    let batch_size = (cfg.ap_batch_size as usize).max(1);
    let mut batch_idx = 0u32;

    while batch_idx < asc.ap_nums {
        // Ctrl+C 中途退出
        if SHUTDOWN.load(Ordering::SeqCst) {
            info!("Interrupted during batch spawning, shutting down...");
            break;
        }
        let batch_end = (batch_idx as usize + batch_size).min(asc.ap_nums as usize);

        for i in batch_idx as usize..batch_end {
            let (ev_tx, ev_rx) = mpsc::channel::<ApEvent>(1024);
            let runner = app::make_runner(
                i as u32,
                ap_map,
                sta_map,
                asc.sta_count,
                &raw_sock,
                &arp_nfy,
                cfg.sta_timeout_secs,
                cfg.sta_max_retry,
                cfg.sta_retry_interval_secs,
                cfg.ka_echo_interval_secs,
                cfg.ka_ka_interval_secs,
                cfg.ka_max_retry,
            );

            let mac_bytes = util::parse_mac_hex(&ap_map[&format!("ap{}_mac", i)]);
            let ip_bytes = util::parse_ip_hex(&ap_map[&format!("ap{}_ip", i)]);

            let reg = registry.clone();
            let ip_reg = ip_registry.clone();
            let nfy = arp_nfy.clone();

            reg.lock().unwrap().insert(mac_bytes, ev_tx.clone());
            ip_reg.lock().unwrap().insert(ip_bytes, ev_tx.clone());

            tokio::spawn(async move {
                app::run_with_retry(
                    i as u32,
                    runner,
                    ev_rx,
                    mac_bytes,
                    ip_bytes,
                    reg,
                    ip_reg,
                    nfy,
                    app::MAX_RETRY,
                    cfg.sta_timeout_secs,
                    cfg.sta_max_retry,
                    cfg.sta_retry_interval_secs,
                    cfg.ka_echo_interval_secs,
                    cfg.ka_ka_interval_secs,
                    cfg.ka_max_retry,
                )
                .await;
            });
        }

        info!(
            "Batch {}/{}: AP {}-{} spawned",
            batch_idx / batch_size as u32 + 1,
            (asc.ap_nums as usize + batch_size - 1) / batch_size,
            batch_idx,
            batch_end as u32 - 1
        );

        batch_idx = batch_end as u32;

        if batch_idx < asc.ap_nums {
            sleep(Duration::from_secs_f64(cfg.ap_thread_interval as f64)).await;
        }
    }

    if batch_idx >= asc.ap_nums {
        info!("All APs launched. Ctrl+C to stop.");
    }

    while !SHUTDOWN.load(Ordering::SeqCst) {
        sleep(Duration::from_millis(500)).await;
    }

    info!("Shutting down...");

    // 1. 先设置关闭标志 —— 所有 send 立即静默返回，不再真正发包
    raw_sock.set_shutting_down();

    // 2. 给子任务一点时间排空当前正在执行的 send（echo/ka 定时器最多 30s 间隔）
    sleep(Duration::from_millis(500)).await;

    // 3. 关闭 fd，唤醒全局 receiver 线程
    raw_sock.shutdown();

    // 4. 恢复网卡混杂模式（exit 不执行 Drop，必须显式恢复）
    raw_sock.restore_if();

    std::process::exit(0);
}
