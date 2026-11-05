# ChronoSync v1.0.2 构建与包校验报告

- 构建时间：`20261105-200000`
- 版本：`1.0.2`
- Rust：`rustc 1.99.1 (58cf8f9c3 2026-10-13)` / Cargo 1.99.1
- 发布构建机：Linux x86_64（Ubuntu 26.04 / glibc 2.43）
- 单文件 WebUI：`web/index.html` 在编译期通过 `include_str!` 嵌入
- 构建元数据：`target`、`variant`、`time` 由 `build.rs` 写入二进制，可通过 `/api/health` 读取

## 1. 构建矩阵与命令

| 包 | Target | Rust flags / toolchain | 产物 |
| --- | --- | --- | --- |
| linux-x86_64-v1 | x86_64-unknown-linux-gnu | `RUSTFLAGS=-C target-cpu=x86-64` | `/tmp/cs102-linux-v1` |
| linux-x86_64-v2 | x86_64-unknown-linux-gnu | `RUSTFLAGS=-C target-cpu=x86-64-v2` | `/tmp/cs102-linux-v2` |
| linux-x86_64-v3 | x86_64-unknown-linux-gnu | `RUSTFLAGS=-C target-cpu=x86-64-v3` | `/tmp/cs102-linux-v3` |
| linux-x86_64-v4 | x86_64-unknown-linux-gnu | `RUSTFLAGS=-C target-cpu=x86-64-v4` | `/tmp/cs102-linux-v4` |
| linux-arm64-armv8a | aarch64-unknown-linux-gnu | aarch64 GNU cross + sysroot | `/tmp/cs102-arm64` |
| windows-x86_64-generic | x86_64-pc-windows-gnu | MinGW-w64 | `/tmp/cs102-windows.exe` |

公共构建命令：

```bash
CHRONOSYNC_BUILD_TIME=20261105-200000 \
CHRONOSYNC_BUILD_VARIANT=<variant> \
cargo build --release --locked [--target <target>]
```

x86 v2–v4 使用对应 `target-cpu`；v1 使用 LLVM 识别的 baseline 名称 `x86-64`。Windows 交叉构建设置 `CC_x86_64_pc_windows_gnu`、`AR_...` 和 `CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER`。ARM64 交叉构建设置 `CC_aarch64_unknown_linux_gnu`、`CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER`、sysroot 和交叉 binutils 的 `LD_LIBRARY_PATH`。

## 2. 二进制校验

| 包 | 字节数 | SHA256 |
| --- | --- | --- |
| linux-x86_64-v1 | 4953496 | `eb1816a3e990ee8b06d267d73a54845d5324379cdc8dfb940c60088ba72a18c2` |
| linux-x86_64-v2 | 4949328 | `36bd905bf4c7286176e179b38df34a0d94f5dd7684f170691064ff9f4cf5390a` |
| linux-x86_64-v3 | 4975424 | `dd4ab78b2d7062f07dfa6a2318900bc4eb2070c04e33359aa0166d436ca6cfca` |
| linux-x86_64-v4 | 4955136 | `e980142dfc285c4f9c87c33fb713b3a4ff15eaac91bcfdd6311801ad54c44dee` |
| linux-arm64-armv8a | 4332160 | `bac43d0ea920fa81bc4b118c37279dbb192ca012f5e1ac698decb2d04a87ab9e` |
| windows-x86_64-generic | 4729344 | `52638404e4d2e5d50494ee2da9029c024f222451d7db73e3de48504fa9a04f02` |

每个 Linux 二进制的 `/api/health` 均在启动后返回：

```json
{"version":"1.0.2","build":{"target":"...","variant":"...","time":"20261105-200000"},"ok":true}
```

Windows 二进制在真实 Windows 主机上返回同样的构建时间和 `generic` 变体；ARM64 二进制在真实 aarch64 主机上返回 `armv8-a` 和相同构建时间。

## 3. 归档校验

| 文件 | 字节数 | SHA256 |
| --- | --- | --- |
| `linux-x86_64-v1` tar.gz | 2985091 | `9b70c0561f0f6c1c1837e12c46c8db9c1ae4cf396bf30b7e2a3615d059daff08` |
| `linux-x86_64-v2` tar.gz | 2982559 | `9d290970725250b773a58fd17c553759a5d7e2eb4a7e15095d2f0582887a2877` |
| `linux-x86_64-v3` tar.gz | 2997112 | `cd38e1e30e83dc370dbafd4affafc518912985606ed276ea5ad31717ba9dfe80` |
| `linux-x86_64-v4` tar.gz | 2986779 | `59cb1cc2f7e315c1b4157ecba58a175beefc5e12f681d0ce6977ec215ad1b0c4` |
| `linux-arm64-armv8a` tar.gz | 2859553 | `d5d5dcfe84e1f2602e61817c7188c526a55e9bfc05adb41f2ea554170167d249` |
| `windows-x86_64-generic` zip | 2926729 | `c257d481e1bba52f6f789f604945fc226808765cba3c6b19316b80fa02e772d3` |
| `source` tar.gz | 958949 | `037dcbe18b26b0956cf466d3e35faa6c90bc871ccbacd9b5f613d0d99c48ba07` |

校验结果：

- 7 个归档全部可解包，目录名包含版本、平台、CPU 变体和构建时间。
- 包内 `chronosync` / `chronosync.exe` 与构建产物 SHA256 一致。
- 所有 Linux 包内二进制可执行并输出版本 `1.0.2`。
- Windows 包内 `.bat` 为 CRLF；Launcher 使用 `--open --auto-port`。
- `sha256sum -c SHA256SUMS` 全部 OK。
- 源码包包含 `Cargo.toml`、`Cargo.lock`、`build.rs`、`src/`、`web/`、`docs/`、`install.sh`、`packaging/`、README、CHANGELOG、LICENSE；不包含 `target/`、`dist/`、日志或发布报告。
- Linux 归档固定 mtime 为 `2026-11-05 20:00:00`（本地时区），无未来时间戳警告。

## 4. 运行库

- Linux x86_64 与 ARM64：动态链接 glibc，`readelf --version-info` 最高要求 `GLIBC_2.39`。
- 构建机 glibc 2.43；更旧系统需在目标机源码构建。
- Windows：MinGW-w64 产物，仅依赖 Windows 系统 DLL，不需要 glibc。

## 5. 安全与完整性

- 构建目录和 `/tmp` 中的测试二进制未写入仓库。
- 发布归档解包后进行文本敏感扫描：token、真实密码值、真实内网 IP、敏感主机名均为 0。
- 二进制额外检查精确 token 和已知密码值，均为 0；通用 40 位十六进制模式不用于二进制，避免把 build ID 误报为 token。
- 发布报告和源码包不包含任何目标机 SSH 凭据。
