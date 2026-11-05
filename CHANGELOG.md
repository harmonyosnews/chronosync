# Changelog

## 1.0.2 - 2026-11-05

### Added

- 设备清单“清除设备”按钮，带确认弹窗；清除浏览器保存的密码，保留运行日志。
- 可选的“记住 SSH 密码”开关；地址、端口和用户名始终保留，密码默认不落盘。
- 局域网扫描与自动填表、无 SSH 标记、扫描范围提示。
- Windows 重复实例检测和实际访问地址提示。
- Windows `--auto-port` 自动端口回退（8686–8696），避免 CLOSE_WAIT 本地连接阻止启动。
- Windows `--auto-port` 对任何绑定错误都继续尝试后续端口，包括 10013/权限拒绝；不再只处理 10048。
- Windows `stop-chronosync.bat` 与改进的启动/停止脚本；脚本模板保存在 `packaging/windows/`。
- 多平台发布：Linux x86_64 v1/v2/v3/v4、Linux arm64 armv8a、Windows x86_64。
- 发布构建元数据和 `BUILD-INFO.txt`。
- 发布页：<https://github.com/harmonyosnews/chronosync/releases/tag/v1.0.2>。

### Changed

- 默认监听 `0.0.0.0:8686`，局域网可访问；仍建议配置访问认证。
- WebUI 冻结为内嵌单文件二进制。
- 日志窗口固定约 10 行并滚动。
- 安装脚本默认监听局域网，自动识别 systemd/OpenRC/procd/AudioSys。
- RTC 写入支持 x86 direct ISA 回退；无 RTC 时跳过并给出明确提示。

### Security

- “记住 SSH 密码”为显式 opt-in，密码以明文存入当前浏览器 localStorage；仅建议个人受信任设备使用。
- 服务端日志和发布报告不记录密码。
- 控制台默认无 TLS；请使用可信内网并设置 `--auth`。
