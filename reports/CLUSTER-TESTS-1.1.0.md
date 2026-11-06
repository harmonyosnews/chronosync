# ChronoSync 1.1.0 集群模式测试记录

| 项目 | 内容 |
| --- | --- |
| 日期 | 2026-11-06 |
| 版本 | 1.1.0（feature 分支） |
| 构建时间 | 最终矩阵 20261106-125500；正式发布 20261106-130000 |
| 原则 | 只读取时；未写入任何生产设备时钟；不记录凭据/真实 IP 清单 |
| 结论 | 9 节点真实矩阵稳定通过；另有 2 台因平台运行库/加载器不兼容排除 |

## 1. 可达性盘点

只测试 TCP 22 + SSH 认证 + 只读命令（hostname/arch/chronosync 是否存在），未安装、未改时钟。

- 12 台目标中 11 台可达并完成认证。
- 1 台生产发布机 TCP 22 超时，已从验证矩阵剔除。
- 已认证主机覆盖 x86_64、aarch64；systemd、OpenRC、其他嵌入式 init。
- 认证方式均为密码；未在目标执行写操作。
- 具体地址/账号不写入本报告，避免形成内网清单。

## 2. 9 节点真实矩阵

拓扑：ARM64 主机作为手动 server；x86 构建机、Euphony、2 台 GentooPlayer、4 台 Diretta 作为 client。所有 client 只设置 `--cluster-seed`，`auto_apply=false`。

结果：

- 9/9 节点 `chronosync 1.1.0` 可运行。
- server `role=server`，外部 NTP `verified=true`。
- 8 个 client 在 10 秒和 20 秒两次采样均保持 `server_id` 不变，`last_error=null`。
- 各 client offset 落在 -301ms 到 +379ms；RTT 0–1ms。
- 全部 `applied=false`，未写入任何目标时钟。
- 跨子网节点（x86 构建机）通过 seed + Unicast 心跳成功加入。

矩阵期间发现并修复一个真实缺陷：

- **问题**：client 收到“当前 server”的后续心跳时，因同 term/同 node_id 不满足“更优 server”条件，未刷新 `last_server_seen`；3.5 秒后误判失联，出现周期性 `未发现可用 server`。
- **修复**：当前 server 的同 node 心跳无条件续租；新增回归测试。
- 修复后 9 节点矩阵稳定，单元测试 31 passed。

## 3. Linux x86_64 / ARM64 本机测试

- 手动 server/client：client 完成 5 次签名采样，`auto_apply=false` 不写时钟。
- 两节点 auto：选出单一 server；kill server 后约 8 秒完成重选。
- 错误共享密钥：client 拒绝 server，0 时间样本。
- server 无有效外部 NTP：client 不连接、不写时钟。
- 非 root + `--auto-apply`：写入被拒绝，返回权限错误。
- ARM64 同机双节点：client `offset=-19ms`、`rtt=0ms`，未写时钟。
- WebUI：Server/Client 徽标正常，console/page error 0。

## 4. 跨主机（ARM server + Windows client）

- Windows client 通过 seed 发现 ARM server，完成签名取时：
  - `offset=295ms`（server 外部 NTP 与 Windows 本地时钟差）
  - `rtt=1ms`
  - `auto_apply=false`，未写时钟
- Windows `/api/health` 返回 1.1.0 和正确 build metadata。
- 最终 Windows 1.1.0 二进制已复跑：8s/22s 均保持同一 server，未再出现心跳超时。

## 5. 兼容性排除

以下节点 SSH 可达，但当前动态 1.1.0 无法运行：

- Innuos：glibc 2.31，低于发布包要求 glibc ≥2.39。
- RoonServer（iStoreOS）：缺少匹配的动态加载器，`not found`。

处理建议：从源码为目标构建、升级系统运行库，或后续提供静态 musl 包。不能把这两台计入“已验证集群节点”。

## 6. 实际安装/卸载

- 在 x86 构建测试 VM 上以 root 执行 `install.sh install --no-start --cluster-mode server`。
- 安装结果：`/opt/chronosync`、`run.sh`、`cluster.key`（0600）、`cluster-state`（0700）、systemd unit 全部生成。
- `install.sh status` 显示 1.1.0；服务未启动（`inactive`），未写时钟。
- 执行 `install.sh uninstall --purge` 后 `/opt/chronosync` 和 unit 均清理。
- 该测试只覆盖 systemd 安装/卸载；OpenRC/procd/AudioSys 仍只做逻辑审查和 dry-run。

## 7. 未完成项

1. 未完成广播阻断、分区脑裂、跨网段自动选举的完整故障矩阵。
2. 自动写入 root 成功的真实时钟验证未执行（非 root 拒绝已验证）。
3. Innuos（glibc 2.31）和 RoonServer（缺少动态加载器）不在动态包支持范围。
4. 静态 musl 包未提供。

## 8. 发布建议

当前 9 节点矩阵是强证据，但 1.1.0 仍需完成：

- 至少一次真实 root 安装/卸载；
- Windows 修复后二进制回归；
- 打包、哈希、G1–G13 全量验收并新建 tag/Release；
- 明确生产默认手动 server，auto 仅实验。

1.0.2 的 tag/Release 资产未修改。
