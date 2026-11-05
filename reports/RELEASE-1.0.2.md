# ChronoSync v1.0.2 发布报告

- 状态：**已发布**
- 目标仓库：`https://github.com/harmonyosnews/chronosync`
- 发布版本：**1.0.2**
- 发布 tag：`v1.0.2`
- 冻结 commit：`a3a37b97999851683d74796f7a3b27d5017d4d4d`
- Release ID：`435153841`
- 构建时间：`20261105-200000`
- 发布方式：源码提交 + GitHub Release 二进制资产
- 发布页：`https://github.com/harmonyosnews/chronosync/releases/tag/v1.0.2`
- WebUI：编译时嵌入主程序，运行时为单文件可执行程序

## 1. 范围

本版本包含以下已实现能力：

1. NTP 标准时间参考，NTP 不可达时默认拒绝写入。
2. SSH 批量/单台校时、本机校时、仅检测模式、RTC 写入和北京时间时区修正。
3. 局域网扫描：自动/自定义 IPv4 网段，TCP 22 + ARP/邻居表，反向 DNS，无 SSH 标记。
4. 设备清单：“清除设备”按钮、确认弹窗、清理浏览器保存的凭据。
5. 凭据保留：地址/端口/用户名持久化；SSH 密码由“记住 SSH 密码”开关控制（默认关闭）。
6. 日志窗口约 10 行视窗、实时刷新、日志下载。
7. Windows 启动器：`--open --auto-port`、重复实例检测、自动端口回退、CLOSE_WAIT/PID 排查提示。
8. 多平台发布：Linux x86_64 v1/v2/v3/v4、Linux arm64 armv8a、Windows x86_64 generic。

## 2. 分波修复记录

| 波次 | 内容 | 状态 |
| --- | --- | --- |
| Wave 1 | WebUI 清除设备、可选密码保留、日志窗口收敛、安全文案同步 | 完成 |
| Wave 2 | Windows 重复启动、10048/CLOSE_WAIT、自动端口回退、启动/停止脚本 | 完成 |
| Wave 3 | 1.0.2 版本统一、构建矩阵、文档/截图、包内元数据、发布材料 | 完成 |
| Wave 4 | 闸门发现的问题修复与复测 | 完成 |

Wave 4 的具体问题和修复：

- 初测发现 Windows `--auto-port` 只在 `WSAEADDRINUSE/10048` 时回退；端口 445 返回 `WSAEACCES/10013` 时直接失败。修复为：启用 `--auto-port` 时，任何绑定错误都继续尝试后续端口。
- Linux 回归：非 root 从 1015 启动，遇到 1015–1023 权限拒绝后成功使用 1024，健康接口正常。
- Windows 回归：445 返回 10013 后成功切换到 446，健康接口正常。
- 最终六个二进制均重新构建，构建时间为 `20261105-200000`。

## 3. 闸门结果

| 闸门 | 结果 | 证据 |
| --- | --- | --- |
| G1 `cargo fmt --check` | PASS | 无差异 |
| G2 `cargo clippy --all-targets -- -D warnings` | PASS | 0 个警告 |
| G3 `cargo test` | PASS | 21 passed / 0 failed |
| G4 WebUI JavaScript 语法 | PASS | `node --check` rc=0 |
| G5 浏览器交互回归 | PASS | 版本页脚、330px 日志视窗、清除设备、密码开关/恢复、扫描弹窗；console/page error 均为 0 |
| G6 `install.sh` 语法与 dry-run | PASS | systemd 识别，安装计划正确，未修改系统 |
| G7 全平台构建与启动 | PASS | 六个平台二进制均启动并返回正确构建元数据 |
| G8 真机只读测试 | PASS | Linux 11/12 可达检查、Windows 1/1、ARM64 原生启动；未写入时间 |
| G9 包内容/CPU 标签/哈希 | PASS | 7 个归档解包通过；二进制 SHA256 与构建产物一致；`sha256sum -c` 全 OK |
| G10 敏感信息扫描 | PASS | 精确 token=0、真实密码值=0、真实内网 IP=0、敏感主机名=0 |
| G11 版本一致性 | PASS | 源码/UI/安装器/README/CHANGELOG/包名/构建信息均为 1.0.2；旧发布号残留=0 |
| G12 自动端口回退 | PASS | Linux 权限错误回退 + Windows 445→446 实测 |
| G13 GitHub 发布与回读 | PASS | `main`/`v1.0.2` 指向提交 `a3a37b9`；Release 435153841 的 8 个初传资产大小与 SHA256 digest 全部匹配 |

完整命令输出和细节见 `reports/gates-1.0.2.log`、`reports/BUILD-1.0.2.md` 和 `reports/REAL-DEVICE-TESTS-1.0.2.md`。

## 4. 构建矩阵

| 包标识 | Rust target | CPU 变体 | 二进制 SHA256 |
| --- | --- | --- | --- |
| linux-x86_64-v1 | x86_64-unknown-linux-gnu | x86-64 baseline | `eb1816a3e990ee8b06d267d73a54845d5324379cdc8dfb940c60088ba72a18c2` |
| linux-x86_64-v2 | x86_64-unknown-linux-gnu | x86-64-v2 | `36bd905bf4c7286176e179b38df34a0d94f5dd7684f170691064ff9f4cf5390a` |
| linux-x86_64-v3 | x86_64-unknown-linux-gnu | x86-64-v3 | `dd4ab78b2d7062f07dfa6a2318900bc4eb2070c04e33359aa0166d436ca6cfca` |
| linux-x86_64-v4 | x86_64-unknown-linux-gnu | x86-64-v4 | `e980142dfc285c4f9c87c33fb713b3a4ff15eaac91bcfdd6311801ad54c44dee` |
| linux-arm64-armv8a | aarch64-unknown-linux-gnu | armv8-a | `bac43d0ea920fa81bc4b118c37279dbb192ca012f5e1ac698decb2d04a87ab9e` |
| windows-x86_64-generic | x86_64-pc-windows-gnu | generic | `52638404e4d2e5d50494ee2da9029c024f222451d7db73e3de48504fa9a04f02` |

构建环境：Rust/Cargo 1.99.1，rustc 提交 `58cf8f9c3`；Linux 交叉编译使用 aarch64 GNU 工具链和 sysroot，Windows 使用 MinGW-w64。WebUI 单文件嵌入。

Linux 动态二进制要求 **glibc ≥ 2.39**；ARM64 包已在真实 ARM64 Debian 主机启动验证。Windows 包不依赖 glibc。

## 5. 发布包与校验和

| 文件 | 大小（字节） | SHA256 |
| --- | --- | --- |
| `chronosync-1.0.2-linux-x86_64-v1-20261105-200000.tar.gz` | 2985091 | `9b70c0561f0f6c1c1837e12c46c8db9c1ae4cf396bf30b7e2a3615d059daff08` |
| `chronosync-1.0.2-linux-x86_64-v2-20261105-200000.tar.gz` | 2982559 | `9d290970725250b773a58fd17c553759a5d7e2eb4a7e15095d2f0582887a2877` |
| `chronosync-1.0.2-linux-x86_64-v3-20261105-200000.tar.gz` | 2997112 | `cd38e1e30e83dc370dbafd4affafc518912985606ed276ea5ad31717ba9dfe80` |
| `chronosync-1.0.2-linux-x86_64-v4-20261105-200000.tar.gz` | 2986779 | `59cb1cc2f7e315c1b4157ecba58a175beefc5e12f681d0ce6977ec215ad1b0c4` |
| `chronosync-1.0.2-linux-arm64-armv8a-20261105-200000.tar.gz` | 2859553 | `d5d5dcfe84e1f2602e61817c7188c526a55e9bfc05adb41f2ea554170167d249` |
| `chronosync-1.0.2-windows-x86_64-generic-20261105-200000.zip` | 2926729 | `c257d481e1bba52f6f789f604945fc226808765cba3c6b19316b80fa02e772d3` |
| `chronosync-1.0.2-source-20261105-200000.tar.gz` | 958949 | `037dcbe18b26b0956cf466d3e35faa6c90bc871ccbacd9b5f613d0d99c48ba07` |

包内统一包含 `BUILD-INFO.txt`、`LICENSE`、`README.md`、`CHANGELOG.md`；Linux 包另外包含 `install.sh`、`packaging/chronosync.service` 和截图；Windows 包包含 `chronosync.exe`、`START-HERE.txt`、`start-chronosync.bat`、`stop-chronosync.bat`。源码包不包含 `target/`、`dist/`、日志或报告，避免把发布构建产物和测试环境信息带入源码。

## 6. 真机测试摘要

详细记录见 `reports/REAL-DEVICE-TESTS-1.0.2.md`。

- Linux x86_64 v1 发布二进制：12 台目标，11 台可达并成功读取偏差，1 台不可达；`dry_run=true`，总耗时 2.585 秒，未写入任何时钟。
- Windows x86_64 generic 发布 exe：1 台真实设备成功只读检查，`dry_run=true`；同时验证 445 被占用时自动改用 446。
- ARM64 发布二进制：在真实 ARMv8 主机上原生启动，健康接口返回 `aarch64-unknown-linux-gnu / armv8-a / 20261105-200000`。
- 浏览器回归：1600×1100 视口下清除设备、密码开关、刷新恢复、扫描弹窗和日志视窗均通过。

报告只保留数量、状态和偏差范围，不包含 SSH 密码、真实内网 IP 清单或目标主机名。

## 7. 安全说明

- “记住 SSH 密码”默认关闭；启用后密码以明文保存在浏览器 localStorage，仅建议个人受信任浏览器使用。
- 关闭开关或执行“清除设备”会删除浏览器保存的密码。
- 服务端日志和发布报告不包含 SSH 密码。
- GitHub token 只由临时脚本从本地权限文件读入内存；不使用命令行参数、不写入仓库、日志或发布资产。
- 对仓库、源码包和归档文本执行的敏感扫描结果：真实 token 0、真实密码值 0、真实内网 IP 0、敏感主机名 0。
- 默认 HTTP 无 TLS；局域网部署应启用 `--auth` 并限制在可信网络。

## 8. 安装与验证

```bash
sha256sum -c SHA256SUMS
tar -xzf chronosync-1.0.2-linux-x86_64-v1-20261105-200000.tar.gz
cd chronosync-1.0.2-linux-x86_64-v1-20261105-200000
sudo ./install.sh --auth admin:'换成强密码'
```

发布页：<https://github.com/harmonyosnews/chronosync/releases/tag/v1.0.2>

## 9. 发布后回填

- 仓库：`harmonyosnews/chronosync`，公开仓库，默认分支 `main`
- 冻结 commit：`a3a37b97999851683d74796f7a3b27d5017d4d4d`
- tag：`v1.0.2`，annotated tag 对象 `d8967c53d04a9d490bc12d123dde5a761a04eba7`，指向冻结 commit
- Release：ID `435153841`，`https://github.com/harmonyosnews/chronosync/releases/tag/v1.0.2`
- 初传资产 8 个：6 个平台包、source 包、`SHA256SUMS`；GitHub 回读的大小和 `sha256:` digest 与本地一致
- 发布报告、构建报告和真机测试报告随后作为 Release 文本资产上传
- 发布后的文档回填提交只修改 `reports/`，不改变已发布的二进制、源码包或标签
- 技术产物与 SHA256 以冻结 commit、Release 资产和 `SHA256SUMS` 为准

发布完成。最终校验未发现 token、真实 SSH 密码、真实内网 IP 清单或敏感目标主机名。
