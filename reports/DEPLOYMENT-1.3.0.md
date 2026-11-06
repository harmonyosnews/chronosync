# ChronoSync 1.3.0 验证机部署记录

- 日期：2026-11-06
- 范围：11 台可达验证机（`.205` 不可达，剔除）
- 版本：1.3.0 静态包
- 时钟策略：`auto_apply=false`，未执行时间写入
- 集群：`.230` server（启用标准 NTPv4 UDP 123）；其余 10 台 client，seed `.230:18686`

## 结果

- 11/11 安装并运行 `chronosync 1.3.0`。
- server `/api/cluster/state`：`role=server`、`ntp123=true`、10 peers。
- 10 个 client 全部 `server_id` 指向同一 server、`last_error=null`。
- 全部 `applied=false`；offset -523ms 到 +424ms；未写时钟。
- 标准 NTP123 在本机测试：48 字节响应、LI=0/VN=4/mode=4/stratum=3。
- 原有 1.2.0 服务已停止、替换二进制并重启；AudioSys 持久目录同步。

## 限制

- NTP123 无 HMAC，只在可信内网开放；默认关闭，本次 server 显式启用。
- 分区矩阵为端口隔离模拟；真实交换机/AP 过滤未覆盖。
- Windows 未纳入这 11 台（Windows 主机单独验证）。
