# ChronoSync v1.4.0 构建与包校验

- 构建时间：`20261106-150000`
- 版本：`1.4.0`
- Linux：musl 静态；x86_64 v1–v4 在 `.133` Debian 11 Docker，ARM64 在 `.230` 原生构建
- Windows：MinGW-w64（`.133` Debian 11 Docker）
- 工具链：x86/Windows `rustc 1.98.1`；ARM `rustc 1.99.0`

## 1. 二进制

| 包 | Target | SHA256 |
| --- | --- | --- |
| x86 v1 | x86_64-unknown-linux-musl | `4e8d8c7c3e5ce85e4b4dcc82b41236ef180d41f6d8115fad6251ce231a535c75` |
| x86 v2 | x86_64-unknown-linux-musl | `32ee34c556982e52fbd53dc37d19e013748bc87f59efcf8326c740cca1c21093` |
| x86 v3 | x86_64-unknown-linux-musl | `4b1490288ba1b4b19992084f817f8cc28b7b2f8e7ac4f81547c95ecfcd56879b` |
| x86 v4 | x86_64-unknown-linux-musl | `2ff60b0e5d4fbc91c14e7f6e11d41565d4bbe428cb8e850a58816dbd2c082898` |
| ARM64 | aarch64-unknown-linux-musl | `df1d01292485c73792deb41f6ac22b9f0a3a3b33aac09e715c6bdfb5f9323305` |
| Windows | x86_64-pc-windows-gnu | `1e9b0130c6eea2b571a79bc3b90e77cd12031497c86cac9462da59c5e65233bd` |

`file`：Linux 均 `statically linked`；`readelf -l` 无 `INTERP`；`ldd` 不是动态可执行文件。Windows 为 PE32+ x86-64。

启动冒烟：x86 v1–v4 在 `.133` 上 `/api/health` 返回 200，target=`x86_64-unknown-linux-musl`、build time=`20261106-150000`、version=`1.4.0`；ARM64 在 `.230` 上同样通过；Windows 在 Windows 主机 health 200、auto-port 18890 可用。

## 2. 归档

| 文件 | 字节 | SHA256 |
| --- | --- | --- |
| linux-x86_64-v1-static | 3196930 | `91fed98735b122b2b6728bcd645ce74b4a92aa8bdd74f32058e1e8bd8be96fe3` |
| linux-x86_64-v2-static | 3192904 | `3b6df73845a8b63f62751ff41bbc8c53b1f7345994db1ae78e3e664b0ebc3af5` |
| linux-x86_64-v3-static | 3203146 | `40afc4f985c2e9ac360ac288626b4dab9a48334cc94bf57f55ed00a0f93fc53a` |
| linux-x86_64-v4-static | 3198319 | `49885174611d2ad996cdb4c6471246cdf6509f42775f93d6e313f06ce1f54db6` |
| linux-arm64-armv8a-static | 3119195 | `a5df98a7f3e17372e910eb25c05fd081b9dbf46754adfbcafa9f87a870230827` |
| windows-x86_64-generic | 3137307 | `70e1fc88725d98479059f0d5565bf60befed7608beecb5dfaf91924199f6157a` |
| source | 1497540 | `6fd7e5c83326d4897a1624277cc16f5e6dbffea7d97ccecbc905f7ccb94af82a` |

`SHA256SUMS`：`5a268fe2d0394f3f3a53893b63225f5046fe09513fd7c929c7314f29d2a3ea2c`；`sha256sum -c` 全 OK；包内二进制与构建产物一致。

## 3. 包校验

- 7 个归档全部可解包；Linux 包内 `chronosync`、Windows 包内 `chronosync.exe` 与构建 SHA256 一致。
- `BUILD-INFO.txt` 包含 1.4.0、build time、target、variant、musl/无 glibc 依赖、WebUI 控制说明。
- Linux 包内二进制 `--version` = 1.4.0。
- Windows bat 为 CRLF；PE32+。
- 包内文档包含 1.4.0 使用手册与 11 台部署口径。
- 发布包不含 target/dist/logs/真实凭据。

## 4. 已知限制

- Windows 已通过 WSL 互操作在 Windows 主机执行 `--version`、`/api/health` 200、`--auto-port` 18889→18890；WebUI 重启返回 501（预期）。
- x86 v2–v4 只做启动冒烟，不承诺覆盖所有 CPU 型号；生产默认推荐 v1。
- 标准 NTPv4 仍无 HMAC；默认关闭。
- 静态包 DNS/NSS 行为与 glibc 不同，已做 11 台真实设备验证。
