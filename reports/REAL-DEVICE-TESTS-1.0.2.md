# ChronoSync v1.0.2 真机只读测试报告

- 测试日期：2026-11-05
- 被测构建：`20261105-200000` / `1.0.2`
- 原则：全部使用 `dry_run=true`，`sync_hwclock=false`，`sync_timezone=false`；**不写入任何生产设备时钟**
- 数据最小化：本报告只保留平台、数量、状态和构建元数据，不记录 SSH 密码、完整内网设备清单或真实主机名

## 1. Linux x86_64 v1

- 测试二进制：`chronosync-1.0.2-linux-x86_64-v1-20261105-200000` 内的 `chronosync`
- 构建元数据：`x86_64-unknown-linux-gnu / x86-64-v1 / 20261105-200000`
- 标准时间：启动时 NTP 验证成功
- 目标数量：12
- 结果：`completed`，11 台 `checked`，1 台 `error`
- 偏差数据：11 台均返回 `before_drift_ms`
- 失败分类：1 台 SSH 目标不可达（网络/路由层，非认证或写入失败）
- 总耗时：2585 ms
- 写入：0 次；所有设备均为只读预检

## 2. Windows x86_64 generic

- 测试二进制：Windows 包内的 `chronosync.exe`
- 构建元数据：`x86_64-pc-windows-gnu / generic / 20261105-200000`
- 健康接口：HTTP 200，`version=1.0.2`，`sync_ready=true`
- 真实设备只读检查：1 台，`checked`，返回偏差，`dry_run=true`
- 写入：0 次
- 自动端口：端口 445 被 SMB 占用并返回 10013；启动器成功切换到 446，健康接口 200；447 未监听
- 启动日志包含实际访问地址和 Windows 防火墙提示

## 3. Linux ARM64 armv8a

- 测试二进制：`chronosync-1.0.2-linux-arm64-armv8a-20261105-200000` 内的 `chronosync`
- 测试主机：真实 aarch64 主机，`uname -m` 返回 `aarch64`
- 构建元数据：`aarch64-unknown-linux-gnu / armv8-a / 20261105-200000`
- 启动结果：原生启动，监听成功，`--version` 返回 `chronosync 1.0.2`
- 健康接口：HTTP 200，`version=1.0.2`，`sync_ready=true`
- 权限：检测到 root/管理员权限
- 写入：0 次；测试后停止进程并删除远程临时二进制和日志
- 远程测试产物已在测试结束时清理

## 4. 浏览器 UI 回归

- 被测页面：最终 v1 二进制内嵌 WebUI
- 视口：1600×1100，无头 Chromium
- 版本页脚：`ChronoSync v1.0.2 · STANDARD TIME VIA NTP`
- 日志视窗：330 px（约 10 行）
- 工具栏顺序：扫描 / 添加设备 / 清除设备 / 下载日志
- 清除设备：空清单时按钮禁用；确认弹窗标题和芯片正常；清除后行数 0、localStorage 中设备和密码键均删除、按钮重新禁用
- 记住 SSH 密码：默认关闭；关闭时密码不进入 localStorage，刷新后密码为空、用户名保留；开启后刷新恢复密码并显示安全提示
- 扫描弹窗：标题“扫描局域网”正常，取消操作正常
- console errors: 0
- page errors: 0

## 5. 安全结论

- 所有设备操作均为只读检查，没有执行 `date -s`、`hwclock -w` 或时区写入。
- 测试使用的凭据仅存在于临时内存/权限 600 的临时脚本中，测试后删除。
- 报告和仓库不包含密码、token 或真实设备清单。
- 唯一失败为 1 台目标不可达，未做重试写入，符合“NTP/连接异常不写入”的安全策略。
