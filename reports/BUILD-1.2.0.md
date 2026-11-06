# ChronoSync v1.2.0 静态构建与包校验报告

- 构建时间：`20261106-133000`
- 版本：`1.2.0`
- 目标：Linux x86_64 v1-v4、Linux arm64 为 **musl 静态**；Windows 为 MinGW 原生包

## 1. 构建矩阵

| 包 | Target | 工具链/环境 | 二进制 SHA256 |
| --- | --- | --- | --- |
| linux-x86_64-v1-static | x86_64-unknown-linux-musl | .133 Debian 11 Docker / Rust 1.98.1 / musl-gcc | `a36b48fdcc7c06e7c26aa9ca1f5f274a75e4acd1af006c8ca4f1ab5b131c8d9f` |
| linux-x86_64-v2-static | x86_64-unknown-linux-musl | 同上 | `688863a3b62ebc91d4daf529bd664cf98574d7bac912dca38d5e069829426963` |
| linux-x86_64-v3-static | x86_64-unknown-linux-musl | 同上 | `1aa5394ca7fa08933d849973acacf028a32c4955416d0cbd851921e692de21bd` |
| linux-x86_64-v4-static | x86_64-unknown-linux-musl | 同上 | `1dd01d1b7253590e233964d4437ae63efca476bacacfd7f6f7fb310386a5e179` |
| linux-arm64-armv8a-static | aarch64-unknown-linux-musl | .230 Armbian/Debian 13 原生 / Rust 1.99.0 / musl-gcc | `8808cc49a64ed7008110e232031b21a272d0f2a7c1b5f184276e20e80281e97d` |
| windows-x86_64-generic | x86_64-pc-windows-gnu | .133 Debian 11 Docker / MinGW-w64 | `94f7758ee8cbd66c7676899900af485df368b0951739181f97dd5990bb570fc5` |

静态 flags：`-C target-feature=+crt-static -C relocation-model=static -C link-arg=-no-pie`。只加 `+crt-static` 会产生 static PIE 并带 `/lib/ld-musl-*` 解释器；本版已用 `readelf` 复核无 `INTERP`。

## 2. 静态验证

- `file`：x86 v1-v4、ARM64 均为 `statically linked`。
- `ldd`：`不是动态可执行文件`。
- 在 Innuos（glibc 2.31）和 RoonServer（iStoreOS、缺少动态加载器）上执行 `chronosync --version`，均返回 `1.2.0`。
- 1.2.0 build metadata：target `*-musl`、variant、time `20261106-133000`。

## 3. 归档与 SHA256

| 文件 | 大小（字节） | SHA256 |
| --- | --- | --- |
| `chronosync-1.2.0-linux-x86_64-v1-static-20261106-133000.tar.gz` | 3163895 | `e24cdf225cb2f9b2e2fa981eb3f8701e354c0fbbfbc5f82d32863d73c7ea228a` |
| `chronosync-1.2.0-linux-x86_64-v2-static-20261106-133000.tar.gz` | 3160158 | `977f5ff96b71565909422f3e00f1a8bbd0ae23e3d6f16ec5ffb1ad4b7716a77b` |
| `chronosync-1.2.0-linux-x86_64-v3-static-20261106-133000.tar.gz` | 3169692 | `e20fedc2b840f74039483a4013b9cdb0c92b42abdd195435a1228069dcf03fa6` |
| `chronosync-1.2.0-linux-x86_64-v4-static-20261106-133000.tar.gz` | 3163463 | `7c3f5e69c25a3e033dafec5c466c9e61fde456fe190679fe04b7ee49f13ef399` |
| `chronosync-1.2.0-linux-arm64-armv8a-static-20261106-133000.tar.gz` | 3082211 | `1fe2c816fcbfb7a397a9759c32081a18ad63616c4fbe800860c521ddf71ac30b` |
| `chronosync-1.2.0-windows-x86_64-generic-20261106-133000.zip` | 3090208 | `a92f67bd142b628a0cfc1ce72923a1935409697aaee80a4ae38fdbb50c424578` |
| `chronosync-1.2.0-source-20261106-133000.tar.gz` | 1029970 | `21a8e383016dc87a30f9f418d43323ca22e37d2b790c8c4bad112b29c2276cf4` |

- 7 个归档解包通过；包内二进制哈希与构建产物一致；`sha256sum -c SHA256SUMS` 全 OK。
- source 包不含 target/dist/logs/reports；包文本敏感扫描 0。
- Windows bat 为 CRLF。

## 4. 运行要求

- Linux 静态包不依赖 glibc；适合 Debian 11/13、Innuos、RoonServer、Buildroot 等。
- Windows 包无 glibc 依赖，使用 Windows 系统 DLL。
- 集群默认 off；生产建议手动 server，auto 仅实验。

## 5. 结论

x86 静态 PIE 问题已修正，Linux 六变体静态构建和 Innuos/Roon 兼容验证通过；可作为 1.2.0 发布资产。
