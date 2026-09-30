# crazy_ap 设计文档

> 本文档描述程序的设计原理和实现细节，可用任意语言（Python/Go/C 等）复现。

---

## 1. 目标

在单台 Linux 机器上模拟数千个 CAPWAP AP 同时上线，对 AC（无线控制器）进行压力测试。

核心要求：
- 一个物理网卡承载所有 AP，无需创建虚拟网卡（macvlan）
- 每个 AP 独立运行完整 CAPWAP 协议状态机
- 每个 AP 可挂载多个 STA（终端）
- 支持 ARP（AC 需要通过 ARP 学到 AP 的 MAC）

---

## 2. 整体架构

```
┌──────────────────────────────────────────────────────────┐
│ 主线程                                                     │
│   1. 读配置 (TOML)，支持 -c 指定文件路径                     │
│   2. 创建 AF_PACKET raw socket，绑定物理网卡                │
│   3. 生成 AP/STA 的 MAC 和 IP 地址表                        │
│   4. 创建 ARP 通知器（Notify），供全局 receiver 广播使用      │
│   5. 启动全局 receiver 线程 (收包 + 路由 + ARP 通知)          │
│   6. 并发批量启动 AP（每批 N 个，间隔 T 秒）                 │
└──────────────────────────────────────────────────────────┘
         │                              │
         ▼                              ▼
┌──────────────────┐    ┌──────────────────────────────┐
│ 全局 Receiver 线程 │    │  AP 协程 × N                  │
│ (OS 线程)         │    │ (tokio task / asyncio)       │
│                   │    │                              │
│ recv_raw() 阻塞读  │    │  ┌────────────────────────┐ │
│   ↓               │    │  │ 事件循环                 │ │
│ 解析 Eth 类型      │    │  │  event_rx.recv()        │ │
│   ├── 0x0806 ARP  │    │  │     ↕                   │ │
│   │   ├── request │    │  │  mpsc channel           │ │
│   │   │   按 target_ip 路由到 AP  │  │     ↕            │ │
│   │   └── reply   │    │  │  状态机 on_event()      │ │
│   │       更新 AC MAC  │  │     ↕                   │ │
│   └── 0x0800 IPv4 │    │  │  exec(动作列表)         │ │
│       CAPWAP 端口? │    │  │     ├─ SendControl      │ │
│       按 dst_mac   │    │  │     ├─ SendArpReply     │ │
│       路由到 AP ───┼────│────→ ├─ NotifyKeepAlive   │ │
│                   │    │  │     ├─ NotifyEcho        │ │
│                   │    │  │     └─ NotifyStaOnline   │ │
│                   │    │  └────────────────────────┘ │
│                   │    │                              │
│                   │    │  ┌────────────────────────┐ │
│                   │    │  │ 子任务                   │ │
│                   │    │  │  ├─ keepalive 定时器     │ │
│                   │    │  │  ├─ echo 定时器          │ │
│                   │    │  │  ├─ STA 上线流水线       │ │
│                   │    │  │  └─ 隧道转发 (tun_fwd)   │ │
│                   │    │  └────────────────────────┘ │
│                   │    └──────────────────────────────┘
└──────────────────┘
```

### 关键设计决策

| 决策 | 原因 |
|------|------|
| 所有 AP 共用 1 个 raw socket | 避免创建 6000 个虚拟网卡，节省资源 |
| 每个 AP 1 个协程（不是 1 个线程） | 6000 OS 线程太重，协程轻量 |
| 收包用独立 OS 线程 | raw socket 的 recv 是阻塞调用，不适合协程 |
| 发包直接 sendto | AF_PACKET 的 sendto 线程安全，多协程可并发发 |
| 纯状态机（无 I/O） | 状态转换逻辑可独立测试 |

---

## 3. 网络层：AF_PACKET Raw Socket

### 3.1 为什么要用 raw socket

普通 UDP socket 绑定一个 IP:Port，每个 AP 需要自己的 socket。6000 AP = 6000 socket。

AF_PACKET raw socket 在二层（以太网）工作，手动构造完整帧，一个 socket 可以模拟任意数量的源 MAC/IP。

### 3.2 创建 raw socket

```c
// C 伪代码
int fd = socket(AF_PACKET, SOCK_RAW, htons(ETH_P_ALL));

struct sockaddr_ll addr = {
    .sll_family   = AF_PACKET,
    .sll_protocol = htons(ETH_P_ALL),
    .sll_ifindex  = if_nametoindex("enp43s0"),
};
bind(fd, &addr, sizeof(addr));
```

Python 等价：

```python
import socket
sock = socket.socket(socket.AF_PACKET, socket.SOCK_RAW)
sock.bind(("enp43s0", 0))
```

### 3.3 发包：手动构造帧

每个 CAPWAP 报文需要构造完整的 Eth + IP + UDP 头：

```
┌──────────────┬──────────────┬──────────────┬──────────────┐
│  Eth (14B)   │  IP (20B)    │  UDP (8B)    │  CAPWAP 载荷  │
├──────────────┼──────────────┼──────────────┼──────────────┤
│ dst_mac (6)  │ Ver=4,IHL=5  │ src_port (2) │  CAPWAP PDU  │
│ src_mac (6)  │ Total Length  │ dst_port (2) │              │
│ EtherType    │ TTL=64       │ Length       │              │
│ =0x0800(IPv4)│ Proto=17(UDP)│ Checksum=0   │              │
│              │ src_ip (4)   │              │              │
│              │ dst_ip (4)   │              │              │
│              │ Checksum     │              │              │
└──────────────┴──────────────┴──────────────┴──────────────┘
```

**关键字段**：
- Eth `src_mac`：每个 AP 用自己的 MAC（不是物理网卡的 MAC）
- Eth `dst_mac`：AC 的 MAC（配置值或 ARP 解析值）
- IP `src_ip`：每个 AP 自己的 IP
- IP `dst_ip`：AC 的 IP
- UDP `src_port`：AP 动态端口（如 10000 + index）
- UDP `dst_port`：5246（控制）或 5247（数据）
- IP checksum：必须正确计算；UDP checksum 可填 0

**Python 发包示例**：

```python
import struct

def build_and_send(sock, iface, src_mac, dst_mac, src_ip, dst_ip,
                   src_port, dst_port, payload):
    # Eth header
    eth = dst_mac + src_mac + b'\x08\x00'

    # IP header
    ip_ver_ihl = 0x45
    ip_tos = 0
    ip_total = 20 + 8 + len(payload)
    ip_id = 0
    ip_flags_frag = 0x4000  # DF
    ip_ttl = 64
    ip_proto = 17  # UDP
    ip_src = bytes(src_ip)
    ip_dst = bytes(dst_ip)

    ip_header = struct.pack('!BBHHHBBH4s4s',
        ip_ver_ihl, ip_tos, ip_total, ip_id, ip_flags_frag,
        ip_ttl, ip_proto, 0, ip_src, ip_dst)

    # IP checksum
    ip_csum = checksum(ip_header)
    ip_header = ip_header[:10] + struct.pack('!H', ip_csum) + ip_header[12:]

    # UDP header
    udp_len = 8 + len(payload)
    udp_header = struct.pack('!HHHH', src_port, dst_port, udp_len, 0)

    pkt = eth + ip_header + udp_header + payload
    sock.send(pkt)
```

### 3.4 IP 校验和

```python
def checksum(data):
    s = 0
    for i in range(0, len(data), 2):
        w = (data[i] << 8) + (data[i+1] if i+1 < len(data) else 0)
        s = (s + w) & 0xFFFFFFFF
    s = (s >> 16) + (s & 0xFFFF)
    s = ~s & 0xFFFF
    return s
```

### 3.5 收包与路由

收包在一个独立 OS 线程中循环：

```python
def receiver_loop(sock, ap_registry, ip_registry, ac_ip):
    while True:
        pkt = sock.recv(2048)
        eth_type = struct.unpack('!H', pkt[12:14])[0]

        if eth_type == 0x0806:    # ARP
            handle_arp(pkt, ap_registry, ip_registry, ac_ip)
        elif eth_type == 0x0800:  # IPv4
            handle_ipv4(pkt, ap_registry)
```

**IPv4 路由**：检查 UDP 端口是否涉及 5246/5247（CAPWAP），按 Eth dst_mac 路由到对应 AP 的 mpsc channel。

**ARP 路由**：见第 5 节。

---

## 4. ARP 处理

### 4.1 为什么需要 ARP

AC 收到 AP 的 CAPWAP 报文后，需要知道 AP 的 MAC 地址来回复。在真实网络中，AC 通过 ARP 解析。

模拟的 AP 必须做到两件事：
1. **每个 AP 启动时发 ARP 请求**，让 AC 学到本 AP 的 MAC（写入 AC 的 ARP 表）
2. 响应 AC 发来的 ARP 请求（AC 主动查询某个 AP 的 MAC）

首批 AP 还需通过 ARP 回复解析 AC 的真实 MAC（可能与配置文件不同）。后续 AP 复用已解析的 AC MAC，
只发 ARP 宣告自己，不等待回复。

### 4.2 ARP 报文格式

```
Eth:  dst_mac(6) src_mac(6) 0x0806
ARP:  htype(2) ptype(2) hlen(1) plen(1) oper(2)
      sha(6) spa(4) tha(6) tpa(4)
```

- `oper`：1=请求，2=应答
- ARP 请求：`tha` 填 0，`dst_mac` 填 `ff:ff:ff:ff:ff:ff`
- ARP 应答：`tha` 填目标 MAC，`dst_mac` 填目标 MAC

### 4.3 启动时 ARP 流程

分为两个阶段：

**阶段一：首批 AP（AC MAC 未解析）**

```
AP 状态机 Init → Timeout（首次） → SendArpRequest
  → 发 ARP: who-has AC_IP? tell AP_IP
  → 进入事件循环，tokio::select! 等四个事件之一：
      ① CAPWAP 报文
      ② 2s 超时（Timeout）
      ③ 子任务异常
      ④ arp_nfy.notified()（ARP 通知） ← 新增

AC 回复 ARP → 全局 receiver 收到 reply：
  → sock.update_dst_mac(ac_mac)  # 保存 AC 真实 MAC
  → arp_nfy.notify_waiters()      # 广播！唤醒所有在等的 AP
  → 所有 AP 收到 ArpResolved 事件 → 立即发 Discovery（用动态 MAC）

2 秒内未收到回复：
  → Timeout → 发 Discovery（用配置文件中的静态 MAC）
```

**阶段二：后续 AP（AC MAC 已解析）**

```
AP 启动 → resolved_dst_mac 已有值
  → 注入 Timeout → SendArpRequest（发 ARP，让 AC 学到本 AP）
  → sleep 500µs（给 AC 一点时间处理 ARP）
  → 注入 ArpResolved → 立即发 Discovery（无等待）
  → 事件循环中 waiting_arp=false，不监听 arp_nfy 分支
```

核心机制：用 `tokio::sync::Notify` 在全局 receiver 线程和所有 AP runner 之间传递
"AC MAC 已解析"信号，实现事件驱动唤醒，替代死等。

### 4.4 响应 AC 的 ARP 请求

```
AC 发 ARP: who-has AP_IP?
  → 全局 receiver 解析 ARP request
  → ip_registry 按 target_ip 查对应的 AP
  → 发 ArpRequest 事件给该 AP
  → AP 状态机: ArpRequest → SendArpReply
  → AP 发送 ARP reply: AP_IP is at AP_MAC
```

需要维护 `ip_registry`（IP → AP event channel 的映射），在创建 AP 时注册。

---

## 5. CAPWAP 协议状态机

### 5.1 状态定义

```
Init → Discovery → Join → ChangeStateRequest → DataCheck → Running → Online
  │                                                                    │
  └────────────────── Failed ←────────────────────────────────────────┘
```

### 5.2 状态转换表

| 当前状态 | 事件 | 动作 | 下一状态 |
|----------|------|------|----------|
| Init | Timeout（首次，retry=0） | 发 ARP Request | Init |
| Init | ArpResolved | 发 Discovery Request | Init |
| Init | Timeout（retry=1） | 发 Discovery Request（ARP 超时，用静态 MAC） | Init |
| Init | Timeout（retry≥2） | 重发 Discovery，count+1 | Init / Failed |
| Init | Discovery Response | 发 Join Request | Discovery |
| Init | Timeout（重试） | 重发 Discovery，count+1 | Init / Failed |
| Discovery | Join Response | 发 Configuration Status | Join |
| Discovery | Timeout | 重发 Join | Discovery / Failed |
| Join | Configuration Status Response | 发 Change State Event | ChangeStateRequest |
| Join | Timeout | 重发 Config Status | Join / Failed |
| ChangeStateRequest | Change State Event Response | 发 KeepAlive (Data) | DataCheck |
| ChangeStateRequest | Timeout | 重发 Change State | ChangeStateRequest / Failed |
| DataCheck | KeepAlive Response | 通知 KeepAlive 定时器 | Running |
| DataCheck | Timeout | 重发 KeepAlive | DataCheck / Failed |
| Running | IEEE 802.11 WTP Config Request | 回复配置 + 通知 STA 上线 | Online |
| Running | Timeout（60s） | 进入 Failed | Failed |
| Online | Timeout（90s） | Echo 丢失，进入 Failed | Failed |
| Online | Echo Response | 刷新 Echo 定时器 | Online |
| Failed | 任何事件 | 回 Init，发 Discovery | Init |

### 5.3 全局事件（任何状态都处理）

| 事件 | 动作 |
|------|------|
| ArpResolved | 立即发 Discovery（Init 状态专用，AC MAC 已解析） |
| Configuration Update Request | 回复 Configuration Update Response |
| Station Configuration Request | 回复 + 通知 STA 下一步 |
| WTP Event Response | 通知 STA 下一步 |
| ARP Request | 回复 ARP Reply |
| Echo Response | 刷新 Echo 定时器 |
| KeepAlive Response | 刷新 KeepAlive 定时器 |

### 5.4 状态内重试

握手阶段（Init/Discovery/Join/ChangeStateRequest/DataCheck）：
- Init 超时时间：2 秒（等待 ARP 回复）
- 其他状态超时时间：3 秒
- 最大重试：5 次
- 超时后先重发当前请求，不切状态
- 超过 5 次 → Failed → 30 秒后回 Init 重来

### 5.5 纯状态机实现

状态机不执行任何 I/O。`on_event(state, event)` 返回 `(next_state, [action])`。

```python
class ApStateMachine:
    MAX_RETRIES = 5

    def on_event(self, event):
        s = self.state
        # 正常流程
        if s == Init and event == DiscoveryResponse:
            self.state = Discovery
            return [SendControl(build_join_request())]
        # ... 其他状态转换 ...

        # 全局事件
        if event.type == ConfigurationUpdateRequest:
            return [SendConfigResponse(build_config_update_response())]
        if event.type == ArpRequest:
            return [SendArpReply(event.sender_mac, event.sender_ip)]

        # 超时重试
        if event == Timeout:
            self.retry_count += 1
            if self.retry_count > self.MAX_RETRIES:
                self.state = Failed
                return [DelayRetry(30)]
            return [SendControl(build_current_request())]

        # 未预期
        return [Noop]
```

---

## 6. AP 运行器

每个 AP 是一个独立协程，包含：

```
事件循环:
  loop {
    // AC MAC 未解析时，额外监听 ARP 通知
    waiting_arp = (resolved_dst_mac == None)

    ev = select {
      event_rx.recv()    → 来自全局 receiver 的 CAPWAP/ARP 事件
      sleep(timeout)     → 按状态动态超时
      sub_err_rx.recv()  → 子任务故障通知
      arp_nfy.notified() → ARP 已解析（仅 waiting_arp=true 时监听）
    }

    actions = state_machine.on_event(ev)

    for a in actions:
      match a:
        SendControl(data) → raw_socket.send(data, dst_port=5246)
        SendData(data)    → raw_socket.send(data, dst_port=5247)
        SendArpRequest    → raw_socket.send_arp_request(src_mac, src_ip, dst_ip)
        SendArpReply(...) → raw_socket.send_arp_reply(...)
        NotifyKeepAlive   → ka_notify.notify()
        NotifyEcho        → echo_notify.notify()
        NotifyStaNextStep → step_notify.notify()
        NotifyStaOnline   → sta_semaphore.release(2)
        DelayRetry(t)     → sleep(t)
  }
```

### 6.1 子任务

- **keepalive 定时器**：收到 notify → 等 30s → 发 KeepAlive（data 通道 5247）
  - 超过 45s 没收 notify → 超时上报
- **echo 定时器**：同上，发 Echo Request（控制通道 5246）
- **STA 上线流水线**：见第 7 节

---

## 7. STA 上线流程

### 7.1 流水线

每个 STA 上线经过以下步骤：

```
STA 上线流水线:

  状态机收到 IEEE 802.11 Config → NotifyStaOnline
    │
    └→ run_sta_launch 被信号量唤醒
       sleep(3s)  # 给 AC 一些时间
       │
       for each STA:
         ├─ 等 1 个信号量 permit
         ├─ sleep(10ms)
         ├─ 构造 802.11 Association Request（通过 Capwap 隧道发送）
         │     │
         │     └→ run_tun_fwd 收到请求:
         │          ├─ 用 Capwap Data 通道封装并发送 802.11 帧
         │          ├─ 等 step_notify → 发 WTP Event (ApInfoGatherReport)
         │          ├─ 等 step_notify → 发 WTP Event (StaIpReport)
         │          └─ 等 step_notify → 释放 1 个信号量 permit
         │                                 （触发下一个 STA）
         └─ 下一个 STA 开始
```

### 7.2 802.11 Association Request 帧结构

```
802.11 MAC Header (24 bytes):
  Frame Control:  0x0000  (Management, Association Request, 无加密)
  Duration:       314     (0x013A)
  Address 1 (RA): BSSID   (AP 的 radio MAC，6 bytes)
  Address 2 (TA): STA MAC (6 bytes)
  Address 3:      BSSID   (同 Address 1)

Frame Body:
  Capability:     0x1401
  Listen Interval: 0x0001
  SSID Element:   Tag=0, Len=4, "open"
  Supported Rates: Tag=1, Len=8, 1,2,5.5,11,6,9,12,18 Mbps
  ... 其他 IE ...
```

**注意**：Address 1 和 Address 3 都必须填 AP 的 radio MAC（BSSID），不能填错。

---

## 8. 地址分配

### 8.1 设计原则

- 所有 MAC/IP 由程序自动生成，不依赖外部 DHCP 或手动配置
- 跨 AP 不重叠，跨机器可配置前缀隔离
- 容量预留足够大（MAC ~16M AP，IP ~65K AP）

### 8.2 生成规则

```
AP MAC:   {ap_mac_prefix:6} + {:06x}(offset)
           offset = base + ap_index × 16
           每个 AP 占 4 个连续 MAC（AP + radio1 + radio2 + radio3）

AP IP:    {ap_ip_prefix:4} + {:04x}(ap_index + 2)

STA MAC:  {sta_mac_prefix:2} + {:010x}(ap_index × 65536 + sta_index)
          {:010x} = 10 位 hex，确保 NEVER 溢出

STA IP:   {sta_ip_prefix:2} + {:06x}(ap_index × 256 + sta_index)
          {:06x} = 6 位 hex，乘数 256 确保不溢出到 7 位（否则 hex::decode panic）
```

### 8.3 MAC 合法性

模拟 AP 的 MAC 必须是**本地管理单播地址**：
- 首字节 bit0 = 0（单播）
- 首字节 bit1 = 1（本地管理，不会和真实厂商冲突）

合法首字节：`0x02, 0x06, 0x0A, 0x0E, 0x12, 0x16, ..., 0xFE`

如果配置的首字节不合法，程序自动修正：`(byte & 0xFC) | 0x02`

---

## 9. CAPWAP 报文构造

### 9.1 报文结构

```
CAPWAP PDU = Preamble + Header + Control Header + Message Elements
```

### 9.2 主要报文类型

| 报文 | 类型值 | 方向 | 用途 |
|------|--------|------|------|
| Discovery Request | 1 | AP→AC | 发现 AC |
| Discovery Response | 2 | AC→AP | AC 响应 |
| Join Request | 3 | AP→AC | 加入 AC |
| Join Response | 4 | AC→AP | 加入确认 |
| Configuration Status Request | 5 | AP→AC | 上报配置 |
| Configuration Status Response | 6 | AC→AP | 配置确认 |
| Configuration Update Request | 7 | AC→AP | AC 下发配置 |
| Change State Event Request | 11 | AP→AC | 状态变更 |
| Change State Event Response | 12 | AC→AP | 状态确认 |
| Echo Request | 13 | AP→AC | 心跳 |
| Echo Response | 14 | AC→AP | 心跳回复 |
| WTP Event Request | 9 | AP→AC | STA 事件上报 |
| WTP Event Response | 10 | AC→AP | 事件确认 |
| IEEE 802.11 WTP Config Request | 3398913 | AC→AP | WiFi 配置下发 |
| Station Configuration Request | 25 | AC→AP | STA 配置 |
| KeepAlive | — | AP→AC | Data 通道保活 |

### 9.3 Discovery Request 示例要素

```
Preamble:
  version=0, type=0 (Request)

Header:
  radio_id=1, wireless_binding_id=1

Control Header:
  message_type=1 (Discovery Request)
  sequence_number=随机

Message Elements:
  - Discovery Type (type=20): 未知 AC
  - WTP Board Data (type=38): 硬件信息
  - WTP Descriptor (type=39): 设备型号、MAC、序列号
  - WTP Frame Tunnel Mode (type=41): 支持的模式
  - WTP MAC Type (type=44): Local MAC
  - IEEE 802.11 WTP Info (type=1048): radio 数量、频段
```

---

## 10. 并发模型

### 10.1 批量启动

AP 不是同时启动，而是分批并发：

```
batch_size = 10, interval = 0.5s

主循环:
  for batch_start in range(0, total_aps, batch_size):
    # 同批次内并发 spawn
    for i in batch_start .. batch_start + batch_size:
      spawn ap_task(i)
    # 批次间隔
    sleep(interval)
```

最后一轮自动截断到 `total_aps`。

### 10.2 通道容量

AP ↔ 全局 receiver 之间用 bounded channel（容量 256）：

```
全局 receiver → try_send(event, ap_channel)
                  if full: drop + warn  # 不阻塞全局 receiver
```

`try_send` 而非 `blocking_send`：防止一个 AP 的 channel 满了阻塞全局 receiver，导致所有 AP 收不到包。

---

## 11. 错误处理与恢复

| 场景 | 处理 |
|------|------|
| 握手超时 | Init 2s / 其他状态 3s 后重试当前请求，最多 5 次 |
| 重试耗尽 | 进 Failed → 30s 后回到 Init 全部重来 |
| keepalive/echo 子任务崩溃 | 子任务上报错误 → 主循环检测 → Timeout → Failed |
| channel 满 | 丢弃事件 + warn（保护全局 receiver） |
| raw socket 发包失败 | 打 warn 日志，不崩溃 |
| ARP 探测无响应 | 使用配置中的 AC MAC 作为 fallback |
| AC MAC 通过 ARP 解析到新值 | 更新 raw socket 的 dst_mac，后续报文自动使用 |
| Ctrl+C 退出 | set_shutting_down() → 所有 send 静默返回 → sleep → shutdown fd → exit |

### 11.1 优雅退出

关闭流程分 4 步，确保不产生 EBADF 刷屏：

```
Ctrl+C
  │
  ├─ 0ms:   set_shutting_down() → send_raw() 检测标志位，静默 return Ok
  ├─ 500ms: 给子任务排空当前正在执行的 send（echo/ka 定时器最多 30s 间隔）
  ├─ 500ms: shutdown() 关闭 fd，唤醒全局 receiver 线程（退出 recvfrom 阻塞）
  └─ 700ms: process::exit(0)，OS 级终止所有残留 tokio task
```

不等待所有 AP 任务自然结束——压测工具不需要优雅下线，`exit(0)` 干净利落。

---

## 12. 实现顺序建议

如果要重新实现，建议按以下顺序：

1. **AF_PACKET raw socket**：创建、发包（手动拼 Eth+IP+UDP）、收包
2. **ARP**：请求构造、应答构造、收包解析
3. **CAPWAP**：Discovery/Join 交换（最简单的一对报文）
4. **状态机**：Init → Discovery → Join → Running，先不带重试
5. **AP 协程**：事件循环 + 全局 receiver 路由
6. **KeepAlive + Echo**：定时器子任务
7. **STA**：Association Request 构造 + WTP Event 上报
8. **重试 + 错误恢复**：分状态超时、重试计数、Failed 状态
9. **配置**：TOML 解析，地址生成
10. **日志**：文件 + 屏幕分层

---

## 13. Python 实现提示

```python
# 依赖
# pip install tomli  (Python<3.11 用 tomli，3.11+ 用 tomllib)

# AF_PACKET socket
import socket
sock = socket.socket(socket.AF_PACKET, socket.SOCK_RAW)
sock.bind(("enp43s0", 0))

# 收包
pkt = sock.recv(2048)
eth_type = int.from_bytes(pkt[12:14], 'big')

# 发包
sock.send(pkt)

# 协程 (asyncio)
import asyncio
async def ap_runner(ap_index, mac, ip, ...):
    event_queue = asyncio.Queue(256)
    # ... 事件循环 ...

# 全局 receiver（独立线程，因为 socket.recv 阻塞）
import threading
def receiver():
    while True:
        pkt = sock.recv(2048)
        # 解析、路由到 asyncio.Queue
threading.Thread(target=receiver, daemon=True).start()
```

### Python 注意事项

- `asyncio.Queue` 的 `put_nowait` 等同于 `try_send`（不阻塞）
- ARP 报文用 `struct` 打包解包
- CAPWAP报文构造注意字节序（大端）
- raw socket 需要 root 权限
- 6000 AP 建议用 `asyncio.create_task` 而非线程
