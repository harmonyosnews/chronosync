# Issues 整理

本文件是 ChronoSync 的本地问题台账。仓库未启用 GitHub Issues 作为唯一入口；如需外部协作，可把这里的 ID 同步为 GitHub Issue，但状态以本文件和证据为准。

## 1. 状态与优先级

- 状态：`OPEN`、`IN_PROGRESS`、`CLOSED`、`ACCEPTED`（风险接受）、`CANCELED`（范围取消）。
- 优先级：`P0` 阻断/严重，`P1` 高，`P2` 中，`P3` 低/优化。
- 关闭必须有证据/提交/验收记录；接受风险必须有补偿措施和批准人。

## 2. 台账

| ID | 优先级 | 状态 | 问题 | 影响 | 证据/下一步 |
| --- | --- | --- | --- | --- | --- |
| ISS-001 | P2 | ACCEPTED | SSH 主机密钥不固定 | 不可信网络可 MITM | 可信内网；后续 known_hosts/pinning |
| ISS-002 | P2 | ACCEPTED | HTTP 明文，Basic 非加密 | 局域网嗅探 SSH 密码 | `--auth` + 内网/回环/反向代理；后续 TLS 指南 |
| ISS-003 | P2 | ACCEPTED | 记住密码明文存 localStorage | 本机其他用户/扩展可读 | 默认关闭、清除设备、显著提示 |
| ISS-004 | P3 | OPEN | 无 CSRF/限流 | 恶意网页/内网滥用 | 评估 Origin 校验、令牌、限流 |
| ISS-005 | P3 | OPEN | Basic 比较非常量时间 | 理论侧信道 | 使用常量时间比较 |
| ISS-006 | P3 | OPEN | 本机 `hwclock` 无 direct ISA 回退 | 部分 x86 本机 RTC 写失败 | 复用远程探测逻辑 |
| ISS-007 | P3 | OPEN | `optional_step_label` 两份近重复 | 维护成本/文案漂移 | 合并为一个函数，保留 missing 文案参数 |
| ISS-008 | P3 | OPEN | `fail_target` 薄包装 | 多余间接层 | 删除，直接调用 `fail_target_with_drift` |
| ISS-009 | P3 | OPEN | `current_epoch_ms` 与 `system_epoch_ms` 重复 | 重复时间实现 | 抽公共 helper 或统一调用 |
| ISS-010 | P3 | OPEN | `ip_from_neighbor_line` 仅测试使用 | 死代码/解析重复 | 让 `add_neighbors` 复用它 |
| ISS-011 | P3 | OPEN | 自动扫描把宽网段压成 /24 | /22 网段边缘主机可能漏扫 | 文档已说明；需求评审后决定是否扩到 /22 |
| ISS-012 | P2 | ACCEPTED | NTP 无认证 | 恶意/错误时间源 | 可信源、多源过滤、失败锁定 |
| ISS-013 | P3 | INFO | 静态 musl 未产出 | 旧 glibc 系统需源码构建 | 按静态编译文档执行；不声称已支持 |
| ISS-014 | P3 | ACCEPTED | 运行状态仅内存 | 重启丢历史状态 | 单次运维工具；日志落盘 |
| ISS-015 | P3 | CLOSED | 自动端口只处理 10048 | 10013 导致 Windows 启动失败 | 改为任意绑定错误回退；Linux/Windows 复测 |
| ISS-016 | P3 | CLOSED | 清除设备/记住密码缺失 | 无法清理/恢复凭据 | UI + 浏览器回归通过 |
| ISS-017 | P3 | CLOSED | 版本/包时间戳漂移 | 包名/文档/构建信息不一致 | G11 + 包校验通过 |
| ISS-018 | P3 | CLOSED | 文档重复/无索引 | 事实源分散 | 新增 docs/README 与单一事实源规则 |
| ISS-019 | P3 | CANCELED | 24 小时长稳测试 | 无 | 项目决定取消；不列为交付/闸门 |
| ISS-020 | P2 | CLOSED | 发布资产不可回读 | 无法证明发布完整 | GitHub API/公开下载 digest 校验通过 |

## 3. 重点问题说明

### ISS-001 主机密钥

当前 `check_server_key` 恒为 true，目的是兼容大量出厂设备。风险是中间人。补偿：只在可信管理网使用；高安全环境等待 pinning 版本。不得把“已接受”写成“已修复”。

### ISS-002 HTTP/Basic

控制台默认 `0.0.0.0:8686`，Basic 只做访问控制。生产至少 `--auth`，更敏感环境 `--bind 127.0.0.1` + SSH 隧道或 TLS 反向代理。文档已多处提示。

### ISS-004/005 安全增强

CSRF/限流/常量时间比较属于新版本设计，不应在文档提交中顺手改代码；需要测试、兼容性评估和版本发布。

### ISS-007–010 代码债务

均为低风险去重，不影响当前行为；计划在新版本统一处理，避免 1.0.2 资产漂移。

### ISS-019 长稳测试

已取消。任何文档、验收、发布说明不得再写“24h 待跑/通过”。如果未来恢复，另立测试计划和闸门。

## 4. 关闭/接受记录

| 日期 | ID | 决定 | 依据 |
| --- | --- | --- | --- |
| 2026-11-05 | ISS-015 | CLOSED | Windows 445→446、Linux 1015→1024 复测 |
| 2026-11-05 | ISS-016 | CLOSED | 浏览器回归 G5 |
| 2026-11-05 | ISS-017/020 | CLOSED | G9/G11/G13 |
| 2026-11-06 | ISS-018 | CLOSED | docs 索引与权威源规则 |
| 2026-11-06 | ISS-019 | CANCELED | 项目决定取消 24h 长稳 |
| 2026-11-06 | ISS-001/002/003/012/014 | ACCEPTED | 可信内网 + 操作补偿；见审计报告 |

## 5. 新问题录入模板

```text
ID:
标题:
优先级: P0/P1/P2/P3
状态: OPEN/IN_PROGRESS/CLOSED/ACCEPTED/CANCELED
发现版本/日期:
影响:
证据:
根因:
建议修复:
补偿措施:
关闭条件:
```

新增问题后同步更新 [审计报告](审计报告.md) 的相关章节；涉及闸门的，更新 [纪律与闸门](纪律与闸门.md)。
