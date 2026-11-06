# ChronoSync v1.3.0 构建与包校验

- 构建时间：`20261106-140000`
- Linux：musl 静态；x86_64 v1–v4 在 `.133` Debian 11 Docker，ARM64 在 `.230` 原生构建
- Windows：MinGW-w64（`.133` Debian 11 Docker）

## 二进制

| 包 | Target | SHA256 |
| --- | --- | --- |
| x86 v1 | x86_64-unknown-linux-musl | `7375071aae3e1bccbd01feb9e04fcd6d4d29c8f33bfefe5e2c5fdc07dc1a208e` |
| x86 v2 | x86_64-unknown-linux-musl | `44a40154d306a08b57a7c47f5d5999f14bdcbb798327f0ba84ac01b6b979f42c` |
| x86 v3 | x86_64-unknown-linux-musl | `9c0fab942fb549c2c2f617a2e30583ca6f76de00c4a23f333d37b3d35d6b17d8` |
| x86 v4 | x86_64-unknown-linux-musl | `baacebcf5c8278f30c22b00c8c1288810d3ca22c03fe295f5ced79e6c1944089` |
| ARM64 | aarch64-unknown-linux-musl | `825be2cbc2e8b7fb9fa3d842326e7709025f999a5f959f88174895425735cf61` |
| Windows | x86_64-pc-windows-gnu | `22a98b80a79757d24b8e116789f8b1b028b0aab6ab850384fa454f11a3bdbcd9` |

`file`：Linux 均 `statically linked`；`ldd` 非动态。NTPv4 `--cluster-ntp123` 默认关闭。

## 归档

| 文件 | 字节 | SHA256 |
| --- | --- | --- |
| linux-x86_64-v1-static | 3171968 | `25d6b867278fca288dd5f9c75cfa879be5c0d7aa59688ad4b19bcc2d07c65d95` |
| linux-x86_64-v2-static | 3167983 | `f725e303741458206e6ed86ce6d2dc078809b37606704171e4334e84ddcdd4b2` |
| linux-x86_64-v3-static | 3176981 | `3ec9fa38e65d82a648102e2b04c0b5c6e78c85f9496d832915e563396365f3e8` |
| linux-x86_64-v4-static | 3172419 | `e173cfe46df7a6ef0a5477e24e911f33464499a7dbc452ed39697424cda71a13` |
| linux-arm64-armv8a-static | 3091296 | `29e938dac2745e24bc0b0d53919757b5057bfad45d341e07b49d319bb65dad98` |
| windows-x86_64-generic | 3098512 | `88463a48cc928e911c0a98a8f7d2a99e7b82c6e09944329c99a7afe4833a3d6a` |
| source | 1185238 | `ca8b14abf0c20454637167202e6ca931e3f955cc2618f1e7319aaf44b8c27a99` |

`sha256sum -c SHA256SUMS` 全 OK；包内二进制与构建产物一致；源码包含 `scripts/cluster_matrix.py`。
