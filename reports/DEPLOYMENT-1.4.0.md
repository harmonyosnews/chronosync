# ChronoSync 1.4.0 验证机部署记录

- 日期：2026-11-06
- 版本：1.4.0（Linux 静态 / ARM64 静态）
- 范围：11/11 验证机
- 方式：**先卸载，再重新安装**；`--no-start` → 恢复 node-id/key → 启动
- 绑定：全部改为 `0.0.0.0:8686`
- 时钟策略：`auto_apply=false`，未执行时间写入

## 1. 部署方式

每台主机执行：

1. 读取旧 `install.conf`、`cluster.key`、`cluster-state/node-id`，复制到 `/run` 私有目录（0700）。
2. `install.sh uninstall`（保留日志；不做 `--purge`）。
3. 解包 1.4.0 包，执行 `install.sh install --no-start --bind 0.0.0.0 ...`，带原集群模式/ID/端口/seed/NTP123 参数。
4. 恢复 `cluster.key`、node-id；无认证环境无需恢复 auth。
5. 按平台启动：systemd、OpenRC、procd、AudioSys `chronosyncctl`。
6. 校验 `/api/health`、`/api/cluster/state`、`install.conf` 和二进制版本。
7. 成功后删除临时 key/node-id/包；失败则保留现场。

4 台主机无法从 WSL 直达，通过 `.230` 的 SSH 本地端口转发完成，操作方式与直连一致。

## 2. 结果

| 项目 | 结果 |
| --- | --- |
| 部署数量 | 11/11 |
| 版本 | 全部 `chronosync 1.4.0` |
| 角色 | 1 server + 10 client |
| server | ARM 主机，`ntp123=true`、`ntp_verified=true`、peers=10 |
| client | 全部 `server_id` 指向同一 server，`last_error=null` |
| 写入 | 全部 `applied=false`，未写目标时钟 |
| 绑定 | 全部 `BIND='0.0.0.0'` |
| 集群身份 | node-id 保持不变；client 重装后重新跟随原 server |
| 平台 | systemd / OpenRC / procd / AudioSys 均启动成功 |

## 3. 1.4.0 功能真机验证

- **NTPv4 热开关**：server 本机 UDP 123 请求返回 48 字节，LI=0/VN=4/mode=4/stratum=3；关闭后超时；重新开启后恢复。
- **程序重启**：x86 client HTTP 202，`instance_id` 变化，重启后仍为 1.4.0，systemd 服务保持。
- **手动 Server 接管**：x86 client 提升后 term=4，原 server 降为 client 并跟随；取消后原 server 以 term=5 重新接管，client 跟回。
- **竞态修复**：首次真机接管发现旧 server 在角色切换瞬间发起 client 采样，失败后留下过期 `last_error`；修复为采样结束前校验 `role=client` 且 server node_id 仍匹配。重新构建/部署后接管-回落全程 `last_error=null`。
- **偏差列**：本地浏览器 + SSH 测试替身验证 `+7 ms` 残余偏差渲染；真机未执行写入。

## 4. 限制

- 真机写入未执行；需要维护窗口和明确授权。
- 4 台主机通过 `.230` 隧道部署，未验证重启后 WSL 直连恢复。
- Windows 未做运行测试；包为 MinGW 构建，WebUI 重启在 Windows 返回 501。
- NTPv4 无 HMAC；默认关闭，只在可信内网开启。

## 5. 结论

1.4.0 已在 11/11 验证机按“卸载→重新安装”完成部署，集群与热控制功能通过真机只读验证；未写时钟。
