# ChronoSync 1.4.0 WebUI 控制与偏差列验证

- 日期：2026-11-06
- 版本：`1.4.0`
- 范围：NTPv4 热开关、手动 Server、程序重启、设备清单偏差列、默认安装绑定地址
- 环境：Linux x86_64 开发构建；loopback 双端口集群；独立 SSH 测试替身
- 时钟策略：未写本机或真实目标时钟；SSH 测试替身只返回固定标记

## 1. 实现摘要

- WebUI 新增“集群控制”条：NTPv4 服务开关、手动指定本机为 Server、重启程序。
- NTPv4 监听器支持运行中绑定/释放；关闭后 UDP 端口立即不再响应，重新开启可再次绑定。
- 手动 Server 使用已观察 term + 1 接管；取消后 auto 回发现阶段、固定 server 降为 client。
- 运行设置写入日志目录 `runtime.json`（0600），重启后继续生效。
- 程序重启在 Linux/Unix 用原地 `exec`；兼容原子替换后 `/proc/self/exe` 的 `" (deleted)"` 路径。
- 设备清单新增“偏差”列：检测显示当前偏差；成功写入后显示带符号残余偏差。
- `POST /api/cluster/settings`、`POST /api/system/restart`；`/api/health` 增加 `instance_id`。

## 2. 自动化与 API 验证

安装器源码的 `BIND_HOST` 原值已经是 `0.0.0.0`；本轮修正 README 中仍把 `--bind 127.0.0.1` 当作默认安装示例的写法，并在安装器变量处加注释防止回退。已有 `install.conf` 为 `127.0.0.1` 的机器仍需重新执行安装器（不传 `--bind 127.0.0.1`）才会改写 `run.sh` 并重启服务。

| 项目 | 结果 |
| --- | --- |
| `cargo fmt --check` | PASS |
| `cargo clippy --all-targets -- -D warnings` | PASS，0 warning |
| `cargo test` | PASS，39 passed |
| WebUI JS `node --check` | PASS |
| 安装器 `sh -n` + `--dry-run` | PASS；监听地址 `0.0.0.0:8686` |
| 文档链接 | PASS，126 个相对链接 0 缺失 |
| 敏感扫描 | PASS，token/私钥/真实内网 IPv4/AWS key = 0 |
| NTPv4 开启后独立请求 | 48 字节，LI=0/VN=4/mode=4/stratum=3，origin 回显 |
| NTPv4 关闭后请求 | 超时，无响应 |
| 端口冲突开启 | HTTP 409；`ntp123=false`，错误保留，设置不落盘 |
| 手动 Server 开启 | `role=server`、`manual_server=true`、term 递增 |
| 手动 Server 取消 | `role=client`、`manual_server=false` |
| 启动模式 `server` 无运行设置 | `manual_server=true`、`role=server` |
| 持久化 `manual_server=false` 后重启 | 启动后降为 client，符合覆盖语义 |
| 双节点手动接管 | B 提升后 term=2，A 降为 client 并跟随 B |
| 取消接管回落 | B 降为 client 后 A 以 term=3 重新接管，B 跟随 A |
| 重启 | HTTP 202；`instance_id` 变化，版本 1.4.0，设置保持 |
| 原子替换后重启 | PASS；`/proc/self/exe` 为 `(deleted)` 时仍执行同路径新二进制 |

## 3. 浏览器验证

Chromium 1600×1100 与 390×900：

- 页脚显示 `ChronoSync v1.4.0`。
- 集群控制条正确显示 NTPv4 端口、角色、term、peers 和错误提示。
- 手动 Server 弹确认框；确认后徽标变为 Server，取消后回 Client。
- NTPv4 开关热切换；绑定失败信息可见。
- 重启确认后页面等待新 `instance_id` 并自动刷新；刷新后两个开关状态保持。
- 设备表头包含“偏差”；未执行时显示 `—`。
- 独立 SSH 测试替身返回目标 +7ms，页面成功态显示 `+7 ms`，tooltip 为“校时后残余偏差（设备时间 - 目标时间）”。
- 桌面与 390px 移动端无横向溢出；console/page error = 0。
- 分波修复：发现 1200–1366px 宽度下新增偏差列导致设备表被 `overflow:hidden` 裁切；将双列布局断点前移到 1400px、紧凑列宽扩展到 1500px。复测 1600/1366/1280/1200/1024/820/390，设备表 `scrollWidth == clientWidth`，无裁切。

截图（本地证据目录，默认不入库）：

- `gui-test-screenshots/t1_initial.png`
- `gui-test-screenshots/t2_device_row.png`
- `gui-test-screenshots/t3_manual_confirm.png`
- `gui-test-screenshots/t4_manual_on.png`
- `gui-test-screenshots/t5_ntp_on.png`
- `gui-test-screenshots/t6_restart_confirm.png`
- `gui-test-screenshots/t7_after_restart.png`
- `gui-test-screenshots/t8_mobile.png`
- `gui-test-screenshots/t9_after_sync_offset.png`
- `gui-test-screenshots/t10_1280_layout.png`

## 4. 未执行 / 限制

- 未构建 1.4.0 六平台静态包；本机缺少 musl 交叉编译器，ARM 原生构建机和 `.133` 构建环境未在本轮接入。
- 未做 1.4.0 真机部署；远程验证机访问凭据不在本轮上下文。
- 偏差列写入路径用独立 SSH 测试替身验证，不是 G8 真机证据。
- WebUI 重启仅 Unix；Windows 返回 501，需使用启动/停止脚本或服务管理器。
- 手动 Server 的脑裂/接管用双节点 loopback 脚本验证；真实多节点接管仍需真机矩阵。
- 复现：`python3 -I scripts/manual_takeover_test.py --binary target/debug/chronosync`。
- 1.3.0 Release 资产未改写；1.4.0 尚无 Release。

## 5. 结论

1.4.0 功能已通过本地与 11 台真机只读验证；发布资产以 Release 回读为准。
