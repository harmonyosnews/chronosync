# Changelog

## 1.2.0 - 2026-11-06

### Added

- Linux x86_64 v1/v2/v3/v4 与 ARM64 改为 **musl 静态链接**，不依赖目标 glibc。
- x86_64 变体在 `.133` 的 Debian 11 Docker 构建；ARM64 在 `.230` 原生 musl 构建。
- 静态 PIE 链接修正：`relocation-model=static` + `-no-pie`，产物无动态加载器。
- Innuos（glibc 2.31）与 RoonServer（缺少动态加载器）已用静态包验证可执行。
- 增加静态兼容验证与部署文档；Windows generic 同步到 1.2.0。
- 11 台可达验证机完成 1.2.0 静态安装并启动：1 server + 10 client，`auto_apply=false`，未写时钟。

### Changed

- 发布包命名增加 `static` 标签；BUILD-INFO 记录 musl target 和“无 glibc 依赖”。
- 版本提升到 1.2.0。

### Known Limitations

- 集群默认 off；生产建议手动 server，auto 仅实验。
- 首版单 L2，不使用标准 NTP 123。
- 广播阻断/分区脑裂完整矩阵未完成。
- Windows 仍为 MinGW 原生包，不是 musl 静态。

## 1.1.0 - 2026-11-06

### Added

- `--cluster-mode off|server|client|auto`、集群 ID、node_id、发现/时间端口、seed、`--auto-apply` 等 CLI。
- `src/cluster/`：配置/身份、HMAC-SHA256 控制消息、UDP 发现、选举/租约、NTP-like 时间服务、client 采样与安全写入。
- `/api/cluster/state` 和 WebUI 集群状态徽标。
- 安装器集群参数、共享密钥生成/管理员预置、服务启动参数和防火墙提示。
- 后台 NTP 周期刷新；参考年龄超过 5 分钟的 server 停止供时。
- 版本提升到 1.1.0；Linux v1–v4、Windows generic 启动验证。
- ARM64 双节点 server/client、ARM server + Windows client 跨主机取时验证通过。
- 9 节点真实矩阵稳定通过（ARM server + 8 client，10s/20s 两次采样保持同一 server）。
- 修复 client 对当前 server 心跳未续租导致周期性误判失联的缺陷；新增回归测试。
- 本地验证：手动 server/client、两节点自动选举、server 退出重选、错误密钥/未验证 NTP 拒绝、非 root 自动写入安全拒绝。
- 12 台真实目标可达性盘点：11 台可达，1 台生产发布机不可达。

### Documentation

- 新增根目录 `使用手册.md`、`项目管理与排期.md`、`工程纪律.md`、`AGENTS.md`。
- 新增 `docs/README.md` 文档索引，明确 API、闸门、排期、证据的单一事实源，减少重复维护。
- 新增 `docs/集群模式详细设计与排期.md`：1.1.0 Server/Client 自动选举的协议、状态机、安全、测试与排期。
- 完成仓库科学性/逻辑性/鲁棒性/归一化/冗余审计；代码去重候选登记到 Issues。
- 24 小时长稳测试按项目决定取消，不列入本版交付或闸门。
- README 增加文档地图、集群说明和项目结构。

### Known Limitations

- Innuos（glibc 2.31）和 RoonServer（缺少动态加载器）不能运行当前动态包。
- systemd root 安装/卸载已实测；OpenRC/procd/AudioSys 仍只做 dry-run/逻辑审查。
- 未完成广播阻断、分区脑裂、跨网段自动选举的完整故障矩阵。
- 静态 musl 包未提供。

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
