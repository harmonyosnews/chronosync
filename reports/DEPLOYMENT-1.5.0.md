# ChronoSync 1.5.0 验证机部署记录

- 日期：2026-11-06
- 版本：1.5.0（Linux 静态 / ARM64 静态）
- 范围：11/11 验证机
- 方式：**先卸载，再重新安装**；保留 logs、cluster.key、node-id、install.conf 参数
- 绑定：`0.0.0.0:8686`
- 时钟策略：全部 `auto_apply=false`，未执行时间写入

## 1. 结果

| 项目 | 结果 |
| --- | --- |
| 部署数量 | 11/11 |
| 版本 | 全部 `chronosync 1.5.0` |
| 角色 | 1 server + 10 client |
| server | ARM 主机，term=1、peers=10、`ntp123=true` |
| client | 全部 `server_id` 指向同一 server，`last_error=null` |
| auto_apply | 全部 `false` |
| 写入 | 全部 `applied=false` |
| 绑定 | 全部 `BIND='0.0.0.0'` |
| node-id | 全部保持不变 |
| 平台 | systemd / OpenRC / procd / AudioSys 均启动成功 |

4 台主机通过 `.230` SSH 端口转发部署，其余直连；操作方式相同。

## 2. 1.5.0 功能真机验证

- WebUI 返回 1.5.0；控制条显示 auto_apply 开关、阈值 100ms、最大步长 5000ms、大步长拒绝。
- 在 x86 client 上调用 `POST /api/cluster/settings {"auto_apply":true}`，view 立即变为 true，`runtime.json` 写入 `"auto_apply": true`；随后立即关闭为 false，未跨越取时周期，未写时钟。
- 所有验证机部署后 `auto_apply=false`；未在真机开启自动写入，避免无维护窗口写入。
- NTPv4/手动 Server/重启接口沿用 1.4.0 验证，代码未改；server 仍返回 48B LI0/VN4/mode4/stratum3（1.4.0 真机证据）。

## 3. 二进制一致性

- x86 主机安装 `808a730d...`（v1 static）。
- ARM server/client 安装 `0f700c32...`（ARM64 static）。
- 与 1.5.0 Release 包内二进制一致。

## 4. 限制

- 真机未启用自动写入；实际写入路径由单元策略测试和本地“阈值内不写”集成测试覆盖。
- 自动写入只影响 client 本机时钟，不会写设备清单中的 SSH 目标。
- Windows 未部署到验证机；仅做 Windows health 验证。

## 5. 结论

1.5.0 已按“卸载→重新安装”部署到 11/11 验证机，默认关闭自动写入，集群与 WebUI 开关状态正常。
