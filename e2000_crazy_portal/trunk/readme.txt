1、目录结构：

    ├── config
    │   └── portal_con
    ├── e2000_crazy_portal

    在config/portal_con中是该软件与portal服务器之间的配置
    e2000_crazy_portal是二进制文件，在linux使用sudo ./e2000_crazy_portal，若无法使用，使用chmod 777 来提升权限。

2、配置参数详解：
    [CRATE_IFACE]
    # 创建子接口：1, 删除子接口：0，删除子接口失败则直接重启也可以恢复
    # 要运行程序这里必须为1
    crate_iface = 1
    iface_name = "enp1s0" # 这里在linux 上使用ifconfig查看接口名称/或者使用ip addr 查看

    [ReAuth]
    # enable_reauth ? yes : 1 ; no : 0 
    portal_reauth = 0 # 当前程序不支持，

    [GET_IP]
    # 动态还是静态遍历IP地址
    # 动态获取IP地址：1，静态获取IP地址：0 动态没做
    dynmaic_ip = 0
    # 使用IPv6
    use_ipv6 = false
    # 静态IPv4地址，起始IP地址
    start_ip_v4 = "173.0.0.20"
    # 静态IPv4地址，子网掩码
    static_mask_v4 = "255.0.0.0"
    # 静态IPv4地址，默认网关
    static_gw_v4 = "173.0.0.1"
    # 静态IPv6地址，起始IP地址
    start_ip_v6 = "2001:db8::1"
    # 静态IPv6地址，前缀长度 only 64
    static_prefix_v6 = 64
    # 静态IPv6地址，默认网关
    static_gw_v6 = "2001:db8::ff"

    [PORTAL_SERVER]
    portal_ip_or_ipv6 = "100.100.1.34"
    # 端口默认为80
    portal_port = 80 # 配置的重定向端口

    [PORTAL_USER]
    # 用户数量
    portal_user_num = 2
    # 用户名共同部分
    portal_user_head = "zjb"
    # 用户名后缀 后缀会一直增加和USE_NUM个数相同
    portal_user_tail = 1 # 第一个就为zjb1 第二个为zjb2
    # 密码
    all_password = "a123456" # 只能使用相同密码
    # 用户与用户之间认证间隔 单位s # 这个程序是异步的，这里可以调节用户之间的认证速度，目前仅支持秒为单位
    next_user_time = 0

    [REDIRECT_IP]
    # 重定向IP 能访问的http
    redirect_ip_or_ipv6 = "100.100.1.32"

    [STA_MAC_HEADER] 
    # mac地址前缀，可以自己取目前支持 0xFFFFFF个终端，有需求可以开
    sta_mac_header = "000174"

3、更改内核参数：
    备份内核参数 sudo cp /etc/sysctl.conf /etc/sysctl.conf.bak
    删除现有的内核参数：sudo rm /etc/sysctl.conf
    更新内核参数：创建sysctl.conf且填入如下参数：

    net.ipv4.ip_local_port_range = 1024 65535
    net.core.rmem_max = 16777216
    net.core.wmem_max = 16777216
    net.ipv4.udp_rmem_min = 87380
    net.ipv4.udp_wmem_min = 65536
    net.core.netdev_max_backlog = 30000
    net.ipv4.neigh.default.gc_thresh1 = 30000
    net.ipv4.neigh.default.gc_thresh2 = 32000
    net.ipv4.neigh.default.gc_thresh3 = 32768
    net.ipv6.neigh.default.gc_thresh1 = 30000
    net.ipv6.neigh.default.gc_thresh2 = 32000
    net.ipv6.neigh.default.gc_thresh3 = 32768
    net.ipv4.neigh.default.gc_stale_time = 36000
    net.ipv4.conf.all.arp_ignore = 1
    net.ipv4.conf.all.arp_announce = 2
    net.ipv4.conf.all.rp_filter = 2
    net.ipv4.neigh.default.gc_stale_time = 3600
    net.ipv4.neigh.default.base_reachable_time_ms = 3600

    使用 sudo nano etc/sysctl.conf  (或者其他编辑方式)
    创建好后使用sudo sysctl -p  加载内核配置(无报错则成功，能看到刚刚配置的值)
    若创建用户数超过800 必须使用 sudo ulimit -n 65535 来增加文件个数(此配置只在当前terminal生效，更改必须再次执行)。


4、环境配置
    环境配置无硬性要求，确保linux设备能进行portal认证即可。