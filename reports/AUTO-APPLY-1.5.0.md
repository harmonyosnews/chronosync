# ChronoSync 1.5.0 自动写入开关验证

- 日期：2026-11-06
- 版本：`1.5.0`
- 范围：WebUI `auto_apply` 开关、阈值/大步长复用、持久化、GUI/API
- 时钟策略：未写本机/目标时钟；集成测试使用 `max_offset_ms=3600000`，偏差 768ms 落在阈值内

## 1. 实现

- `Cluster` 新增 `auto_apply: AtomicBool`；client 取时循环实时读取。
- `POST /api/cluster/settings` 接受 `auto_apply`，写入 `runtime.json`（0600）；重启后保持。
- `ClusterView` 增加 `max_offset_ms`、`max_step_ms`、`allow_large_step`；WebUI 显示阈值/步长/大步长策略。
- 开启前确认弹窗；仅 `role=client` 可操作；server 上禁用。
- 抽取 `auto_apply_decision`，保持原策略：
  - 关闭：只检测，不写
  - `|offset| <= max_offset_ms`：不写
  - `|offset| > max_step_ms` 且未允许大步长：拒绝写
  - 其余：执行写时钟
- 未新增绕过阈值/步长的参数。

## 2. 验证

| 项目 | 结果 |
| --- | --- |
| `cargo test` | 41 passed（含 2 个策略测试） |
| fmt/clippy | PASS |
| 运行时开关 API | `auto_apply=false → true → restart → true → false` |
| `runtime.json` | 0600；`auto_apply` 持久化；重启回读正确 |
| GUI 确认弹窗 | 显示阈值 100ms、最大步长 5000ms、拒绝大步长 |
| GUI 状态 | 确认后开关为开，控制条显示阈值/步长；关闭后恢复“仅检测” |
| 移动端 | 390px 无横向溢出；console/page error=0 |
| 阈值内集成 | client 偏差 768ms、阈值 3600000ms：`applied=false`，消息“偏差 768ms 在阈值内，不写入” |
| 实际写入 | 未执行；真实部署保持 `auto_apply=false` |

截图（本地证据目录，默认不入库）：

- `gui-test-screenshots/t11_autoapply_confirm.png`
- `gui-test-screenshots/t12_autoapply_on.png`
- `gui-test-screenshots/t13_autoapply_mobile.png`

## 3. 限制

- 真实部署未开启自动写入；需要维护窗口和明确授权。
- 自动写入只影响运行 ChronoSync 的 client 本机时钟，不会通过 SSH 写设备清单中的目标设备。
- WebUI 修改开关不改变启动参数中的阈值/步长；改阈值仍需重启。
