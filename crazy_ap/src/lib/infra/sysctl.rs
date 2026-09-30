//! 系统工具：内核参数调整

use tracing::{info, warn};

/// 设置 socket 缓冲区内核上限
pub fn apply_sysctl(rmem: u32, wmem: u32) {
    let write_sysctl = |path: &str, val: u32| {
        if let Err(e) = std::fs::write(path, val.to_string()) {
            warn!("Failed to set {}={}: {}", path, val, e);
        } else {
            info!("sysctl {} = {}", path, val);
        }
    };
    write_sysctl("/proc/sys/net/core/rmem_max", rmem);
    write_sysctl("/proc/sys/net/core/wmem_max", wmem);
}
