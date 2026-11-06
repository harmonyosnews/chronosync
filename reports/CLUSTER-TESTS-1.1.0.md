# ChronoSync 1.1.0 集群模式测试记录

| 项目 | 内容 |
| --- | --- |
| 日期 | 2026-11-06 |
| 版本 | 1.1.0（feature 分支，未发布） |
| 构建时间 | Linux 20261106-120000；ARM/Windows 20261106-123000 |
| 原则 | 只读取时；未写入任何生产设备时钟；不记录凭据/真实 IP 清单 |
| 结论 | 核心链路通过；发布级多主机故障矩阵和实际 root 安装仍待完成 |

## 1. 可达性盘点

只测试 TCP 22 + SSH 认证 + 只读命令（hostname/arch/chronosync 是否存在），未安装、未改时钟。

- 12 台目标中 11 台可达并完成认证。
- 1 台生产发布机 TCP 22 超时，未认证。
- 已认证主机覆盖 x86_64、aarch64；systemd、OpenRC、其他嵌入式 init。
- 认证方式均为密码；未在目标执行写操作。
- 具体地址/账号不写入本报告，避免形成内网清单。

## 2. Linux x86_64

- 1.1.0 构建启动、`/api/health`、`/api/cluster/state` 正常。
- 两节点 loopback 集群：
  - 手动 server/client：client 发现 server，完成 5 次签名采样，`auto_apply=false` 不写时钟。
  - auto：选出单一 server；client 取得 offset/RTT。
  - kill server：约 8 秒内完成重选；原 client 成为 server。
  - 错误共享密钥：client 拒绝 server，0 时间样本。
  - server 无有效外部 NTP：client 不连接、不写时钟。
  - 非 root + `--auto-apply`：写入被拒绝，返回权限错误。
- WebUI：Server/Client 徽标正常，console/page error 0。

## 3. ARM64

- 真实 aarch64 主机，1.1.0 原生启动。
- 同机双回环节点：
  - server 外部 NTP `verified=true`。
  - client 发现 server，签名取时成功，`offset=-19ms`、`rtt=0ms`，未写时钟。
  - 两端 health/version/build metadata 正确。

## 4. 跨主机（ARM server + Windows client）

- ARM64 server 绑定集群 UDP 端口，Windows 客户端通过 seed 发起发现。
- Windows client 成功发现 server，`server_addr` 正确，完成签名取时：
  - `offset=295ms`（server 外部 NTP 与 Windows 本地时钟差）
  - `rtt=1ms`
  - `auto_apply=false`，未写时钟
- Windows `/api/health` 返回 1.1.0 和正确 build metadata。
- 证明发布/发现/时间请求/响应/HMAC 可跨 x86_64 Windows 与 aarch64 Linux 工作。

## 5. 未完成项

1. 1 台生产发布机 TCP 22 超时，无法做主机级验证（不记录具体地址）。
2. 未在真实 root/systemd 目标执行 `install.sh` 实际安装；仅完成 dry-run、run.sh 渲染、密钥生成测试。
3. 未完成广播阻断、分区脑裂、跨网段自动选举的完整故障矩阵。
4. 未创建 1.1.0 六平台包、SHA256SUMS 和独立 Release。
5. 自动写入 root 成功的真实时钟验证未执行（非 root 拒绝已验证）。

## 6. 发布建议

当前状态适合作为 **1.1.0 发布候选/MVP**，不建议直接替代 1.0.2 生产安装，直到：

- 完成至少一次真实 root 安装/卸载；
- 完成多主机广播阻断/分区测试；
- 明确生产默认手动 server，auto 仅实验；
- 打包、哈希、G1–G13 全量验收并新建 tag/Release。

1.0.2 的 tag/Release 资产未修改。
