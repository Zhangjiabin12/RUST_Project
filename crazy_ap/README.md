# crazy_ap

CAPWAP AP 大规模模拟器 —— 基于 AF_PACKET Raw Socket，单进程模拟数千 AP 同时上线。

## 架构

```
┌─────────────────────────────────────────────────────┐
│ main.rs                                             │
│  ┌──────────┐  ┌──────────────┐  ┌──────────────┐  │
│  │ TOML 配置 │→│ 全局 RawSocket│→│ 并发批量启动AP │  │
│  └──────────┘  │  (AF_PACKET) │  └──────────────┘  │
│                └──────┬───────┘                     │
│         ┌─────────────┼─────────────┐              │
│         ▼             ▼             ▼              │
│   ┌──────────┐ ┌──────────┐ ┌──────────┐          │
│   │  AP#0    │ │  AP#1    │ │  AP#N    │  ...N 个 │
│   │ 状态机    │ │ 状态机    │ │ 状态机    │          │
│   │ +子任务   │ │ +子任务   │ │ +子任务   │          │
│   └──────────┘ └──────────┘ └──────────┘          │
│         │             │             │              │
│         └─────────────┼─────────────┘              │
│                       ▼                            │
│            ┌─────────────────────┐                 │
│            │   全局 Receiver      │ 按 dst_mac 分发 │
│            │   (ARP + CAPWAP)    │                 │
│            └─────────────────────┘                 │
└─────────────────────────────────────────────────────┘
```

- **1 个 AF_PACKET raw socket** 服务所有 AP 的收发，无需 macvlan
- **1 个全局 receiver 线程** 收包并按目的 MAC / IP 路由到对应 AP
- **每个 AP 独立 tokio task**：纯状态机 + keepalive/echo/STA 子任务
- **支持 ARP**：启动时自动解析 AC MAC，运行中响应 AC 的 ARP 请求

## 构建

```bash
# 安装 Rust（如未安装）
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# 编译 release
cargo build --release

# 运行测试
cargo test --lib
```

## 运行

需要 root 权限（AF_PACKET raw socket）：

```bash
# 默认配置文件 config/wtp_con.toml
sudo ./target/release/crazy_ap

# 指定配置文件
sudo ./target/release/crazy_ap -c /path/to/config.toml
```

## 配置

配置文件 `config/wtp_con.toml`（TOML 格式）：

```toml
[thread]
interval = 0.5          # 每批 AP 启动间隔（秒）

[ap_batch]
batch_size = 10          # 每批并发启动的 AP 数（1=串行）

[ac]
mac = "cc:d8:1f:c9:80:34"
backup_mac = "cc:d8:1f:c9:7e:c1"
backup_enable = 0
dest_ip = "2.0.0.1"
dest_ip_backup = "2.255.255.254"
control_port = 5246
data_port = 5247
iface_name = "enp43s0"

[ssid_mode]
ssid = "open"
mode = "open"

[ap]
nums = 6656              # AP 总数
mac_prefix = "ccd81f"    # AP MAC 前缀（6 位 hex，首字节会自动修正为本地管理单播）
ip_prefix = "0200"       # AP IP 前缀（4 位 hex）

[sta]
count = 20               # 每个 AP 的 STA 数量
mac_prefix = "02"        # STA MAC 首字节（2 位 hex）
ip_prefix = "04"         # STA IP 首字节（2 位 hex）
```

### 地址分配规则

| 类型 | 格式 | 容量 | 示例 |
|------|------|------|------|
| AP MAC | `{prefix:6}{:06x}` | ~16M AP | `ced81f010000` |
| AP IP | `{prefix:4}{:04x}` | ~65K AP | `02000002` |
| STA MAC | `{prefix:2}{:010x}` | ~16M AP × 65K STA | `020000000000` |
| STA IP | `{prefix:2}{:06x}` | ~65K AP × 256 STA | `04000000` |

### 多机部署

不同电脑设置不同前缀避免 MAC/IP 冲突：

```toml
# 电脑 A                         # 电脑 B
[ap]                              [ap]
mac_prefix = "ccd81f"             mac_prefix = "ccd81f"
ip_prefix = "0200"                ip_prefix = "1200"

[sta]                             [sta]
mac_prefix = "02"                 mac_prefix = "12"
ip_prefix = "04"                  ip_prefix = "14"
```

### 上线速率控制

```
ap_batch_size × (1 / interval) = AP 速率

batch=10,  interval=0.5s  →  20 AP/s    (6656 AP ≈ 5.5 分钟)
batch=100, interval=0.2s  → 500 AP/s    (6656 AP ≈ 13 秒)
batch=1,   interval=0.5s  →   2 AP/s    (串行)
```

## 日志

- **屏幕**：仅 `INFO` 及以上（关键进度）
- **文件**：`logs/crazy_ap.log.YYYY-MM-DD` 全量日志，每天滚动

```bash
# 查看日志文件
ls -la logs/

# 常用过滤
grep "timeout"     logs/crazy_ap.log*   # 超时重试
grep "Failed"      logs/crazy_ap.log*   # 失败 AP
grep "assoc"       logs/crazy_ap.log*   # STA 关联
grep "retransmit"  logs/crazy_ap.log*   # AC 重传触发
grep "AP6655"      logs/crazy_ap.log*   # 单个 AP 完整日志
grep "all STAs"    logs/crazy_ap.log*   # STA 全部上线
grep "resolved"    logs/crazy_ap.log*   # ARP 解析
grep "channel full" logs/crazy_ap.log*  # 通道满丢包
```

## 状态机流程

```
         ┌── ARP(等回复/2s超时) ──┐
         │                        │
Init ──(发 ARP)──→ (发 Discovery)──→ Discovery ──(发 Join)──→ Join ──(发 ConfigStatus)
  ↑                                    │ 超时3s重发5次              │
  │                                    ↓                            ↓
Failed ←────────── 重试耗尽      JoinResponse              ConfigStatusResponse
  ↑                                                    │
  │ 180 - 240s随机 后重试                         ChangeStateRequest
  │                                              ──(发 ChangeState)
  │                                                    │
  │                                           ChangeStateResponse
  │                                                    ↓
  │                                              DataCheck
  │                                              ──(发 KeepAlive)
  │                                                    │
  │                                           KeepAliveResponse
  │                                                    ↓
  │                                               Running
  │                                              ──(等 IEEE802.11Config)
  │                                                    │
  │                                      IEEE 802.11 Config Request
  │                                                    ↓
  └────────────────────────────────────────────── Online
                                                  STA 上线
```

## 目录结构

```
crazy_ap/
├── Cargo.toml
├── DESIGN.md                  # 设计文档
├── config/
│   └── wtp_con.toml           # 配置文件
├── src/
│   ├── bin/
│   │   └── main.rs            # 入口：组装组件、批量启动、等待退出 (~200行)
│   └── lib/
│       ├── mod.rs
│       ├── app.rs             # 应用编排：全局 receiver、AP 工厂、重试包装
│       ├── ap/
│       │   ├── mod.rs
│       │   ├── state.rs       # 纯状态机（CAPWAP 协议状态转换）
│       │   ├── runner.rs      # AP 运行器：事件循环 + keepalive/echo/STA 子任务
│       │   ├── parse_helper.rs # CAPWAP 轻量报文类型解析
│       │   └── types.rs       # 共享类型定义（StaConfig 等）
│       ├── capwap_packet/
│       │   ├── mod.rs
│       │   ├── capwap_packet_build.rs # CAPWAP 报文构造 + 辅助函数
│       │   ├── tlv.rs                  # TLV 编解码
│       │   └── message_element/        # CAPWAP 消息元素定义
│       ├── config/
│       │   ├── mod.rs
│       │   ├── read_config.rs   # TOML 配置解析
│       │   └── ap_sta_config.rs # AP/STA 地址自动生成
│       ├── infra/
│       │   ├── mod.rs
│       │   ├── logging.rs       # 日志初始化
│       │   ├── sysctl.rs        # 内核参数调整
│       │   └── util.rs          # 地址转换工具
│       └── network/
│           ├── mod.rs
│           └── raw_socket.rs    # AF_PACKET raw socket + ARP 收发
└── logs/                        # 日志文件目录（每天滚动）
```
