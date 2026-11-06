# ChronoSync v1.3.0 发布报告

- 状态：**发布候选，待上传**
- 目标仓库：`https://github.com/harmonyosnews/chronosync`
- 版本：**1.3.0**
- tag：`v1.3.0`
- 构建时间：`20261106-140000`
- 发布页：`https://github.com/harmonyosnews/chronosync/releases/tag/v1.3.0`

## 重点

- 可选标准 NTPv4 服务：UDP 123，默认关闭，需 root/管理员；`--cluster-ntp123` / `--cluster-ntp123-port`。
- 标准 NTP 响应 48 字节，LI/VN/mode/stratum/reference/origin/receive/transmit；只在 server 且参考有效时提供同步时间。
- 分区矩阵 4/4：广播屏蔽/seed、双 server 脑裂、合并、server 失联重选。
- Linux 静态包保持 v1-v4 + ARM64；Windows generic。
- 安装器支持 NTP123 参数。

## 验证

| 项目 | 结果 |
| --- | --- |
| 单元测试 | 32 passed |
| 标准 NTP 123 | LI=0/VN=4/mode=4/stratum=3，offset -13.3ms，rtt 0.3ms |
| 分区矩阵 | 4/4 PASS |
| 静态构建 | x86/ARM `statically linked` |
| 包/哈希 | 7 归档 OK |
| 敏感扫描 | 0 |

## 限制

- 标准 NTP 123 无 HMAC；默认关闭，仅可信内网/防火墙限制开启。
- 分区矩阵为端口隔离模拟，不覆盖真实交换机/AP 隔离和跨网段路由。
- 集群单 L2，广播阻断需 seed；auto_apply 默认关闭。

## 回填

Release ID、tag/commit、资产 digest 待上传后补充。
