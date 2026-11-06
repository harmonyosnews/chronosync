# ChronoSync v1.5.0 构建与包校验

- 构建时间：`20261106-170000`
- 版本：`1.5.0`
- Linux：musl 静态；x86_64 v1–v4 在 `.133` Debian 11 Docker，ARM64 在 `.230` 原生构建
- Windows：MinGW-w64（`.133` Debian 11 Docker）
- 工具链：x86/Windows `rustc 1.98.1`；ARM `rustc 1.99.0`

## 1. 二进制

| 包 | Target | SHA256 |
| --- | --- | --- |
| x86 v1 | x86_64-unknown-linux-musl | `808a730d413113c8de0280e0879950401347d0b9ef17db655e22db11042a3f84` |
| x86 v2 | x86_64-unknown-linux-musl | `7b4963d8b189c414bcaf9ab5e2a12c40bb1292e8cc9988a2b8e546ca39a2f41a` |
| x86 v3 | x86_64-unknown-linux-musl | `0167f71d4d7fe0b191611a38816e5bce036aea54045a8973ee10fdb0cee432fa` |
| x86 v4 | x86_64-unknown-linux-musl | `20fe4102fbc2afdc37507d30e100e7adcf12d66ed40bf71a35abdaf44bf5282a` |
| ARM64 | aarch64-unknown-linux-musl | `0f700c325666bdaa164404145ee1f21c9e680a7e9a9cd0aafaa04615e7c18ba6` |
| Windows | x86_64-pc-windows-gnu | `8b94f152c8f15f7405cf7bbad0e2ec39f1baf37c1148c92e9a33b4a6cebe753a` |

`file`：Linux 均 `statically linked`；`readelf -l` 无 `INTERP`；`ldd` 不是动态可执行文件。Windows 为 PE32+ x86-64。

启动冒烟：x86 v1–v4、ARM64 `/api/health` 200，build time=`20261106-170000`、version=`1.5.0`；Windows 在 Windows 主机 `/api/health` 200。

## 2. 归档

| 文件 | 字节 | SHA256 |
| --- | --- | --- |
| linux-x86_64-v1-static | 3199677 | `b6a25a2a5a7180b1529b2fab53d66ba59b1709d0d22fbabfac603267e488eead` |
| linux-x86_64-v2-static | 3194980 | `563ec7a0be7c57f5ce25efc506f4c5efa92bbb7e03db6c00ce7ac033f7861eb3` |
| linux-x86_64-v3-static | 3204599 | `e1937fd14a3786d4980b9d65915890ddc43c2f3089a05f43751b7b017e8d8208` |
| linux-x86_64-v4-static | 3200169 | `64d35dbb245f7ebfb7db9a9b10b36577aba2aaacb82c42ec8510408dcd24adc9` |
| linux-arm64-armv8a-static | 3122658 | `3b490d767cd0f3e7b183c93022f0481eba4ec227734493c155c92c876b9b2266` |
| windows-x86_64-generic | 3140228 | `b90679e6bd00a3ae97468f881edc6b85124bedcb0fb460acbf74cc6d6ad15fa6` |
| source | 1652887 | `163eb4539f810d94c7492777bd0fa926c723335895579b39dc7cd32a51413173` |

`SHA256SUMS`：`6f09511b7ab6fc9aaad06e7f486f38eb9a15a93ca91fe5f2c69abec6caf21d3f`。

## 3. 包校验

- 7 个归档全部可解包；包内二进制与构建产物一致。
- `BUILD-INFO.txt` 包含 1.5.0、build time、target、variant、musl/无 glibc 依赖、WebUI 自动写入说明。
- Linux 包内 `chronosync --version` = 1.5.0；Windows PE32+、bat CRLF。
- 包内文档含 1.5.0 使用手册与 11 台部署口径。
- 发布包不含 target/dist/logs/真实凭据。

## 4. 已知限制

- Windows 只在 Windows 主机做 health 验证，未做完整 GUI 回归。
- x86 v2–v4 做启动冒烟；生产默认推荐 v1。
- 标准 NTPv4 无 HMAC；默认关闭。
- 自动写入默认关闭；真机未触发时钟写入。
