# ChronoSync v1.1.0 构建与包校验报告

- 构建时间：`20261106-130000`
- 版本：`1.1.0`
- Rust：`rustc 1.99.1 (58cf8f9c3 2026-10-13)` / Cargo 1.99.1
- 构建机：Linux x86_64（Ubuntu 26.04 / glibc 2.43）
- 单文件 WebUI：`web/index.html` 嵌入二进制
- 集群：默认 `off`；手动 server 推荐；`auto` 需显式开启

## 1. 构建矩阵

| 包 | Target | CPU 变体 | 二进制 SHA256 |
| --- | --- | --- | --- |
| linux-x86_64-v1 | x86_64-unknown-linux-gnu | x86-64 baseline | `270408950f4a0846b8196cec97e69daec620e9e3c163115aac99e03616b1a55d` |
| linux-x86_64-v2 | x86_64-unknown-linux-gnu | x86-64-v2 | `8c7b8d54defa90687a8c2080f23ad30f50cf27b604b80f3cbbd7b676b07e1669` |
| linux-x86_64-v3 | x86_64-unknown-linux-gnu | x86-64-v3 | `62bfe8e82da54b1b2eb95e6cac5b9b08992178b928a3c411e82ec50433b67355` |
| linux-x86_64-v4 | x86_64-unknown-linux-gnu | x86-64-v4 | `e68f646b973805e048a10826ba2aee3ca9a30b55975540734d86c8d39c3ea087` |
| linux-arm64-armv8a | aarch64-unknown-linux-gnu | armv8-a | `7cad58b45aea5ccb7092c6b4f941cb644f3ee23c79895a413ba147fe964fd12e` |
| windows-x86_64-generic | x86_64-pc-windows-gnu | generic | `a2dc1ac5c573b792787b1cc7074d2e2cdffc31001e200314cfe26374ae5ec32b` |

Linux 动态二进制要求 **glibc ≥ 2.39**；Windows 不依赖 glibc。

## 2. 归档与 SHA256

| 文件 | 大小（字节） | SHA256 |
| --- | --- | --- |
| `chronosync-1.1.0-linux-arm64-armv8a-20261106-130000.tar.gz` | 3017994 | `06defc66eac1f61f695517f80be399849483948073f4dadb080a320d936a2aaa` |
| `chronosync-1.1.0-linux-x86_64-v1-20261106-130000.tar.gz` | 3149698 | `7c1a61f745024db128ade8df55bb1703119ed65cee6239fed0764956ec8cf100` |
| `chronosync-1.1.0-linux-x86_64-v2-20261106-130000.tar.gz` | 3147294 | `dea8aab7ff4ab2add3b6ed91bf3b13b05a1f6b2319dee665b490e2b38b84d637` |
| `chronosync-1.1.0-linux-x86_64-v3-20261106-130000.tar.gz` | 3162937 | `18f462e22b35366c5693c57cb20981763095d2b0cd73f63f67d3470538c4c914` |
| `chronosync-1.1.0-linux-x86_64-v4-20261106-130000.tar.gz` | 3152431 | `29e5bf3aef47fc44ac1524e4e1a27eae6f89c87bdf372a812b95adbb04ccd411` |
| `chronosync-1.1.0-source-20261106-130000.tar.gz` | 1029171 | `d249def06582f5f5b74735e9ae87994620eb8525d3b7c6ac72c4cab94cb8aa76` |
| `chronosync-1.1.0-windows-x86_64-generic-20261106-130000.zip` | 3103926 | `04c3b09a58aedff6172879a1d13f5cdf3a4c49ab2b8ae8296fe82351e9edf4d3` |

## 3. 包校验

- 7 个归档全部可解包。
- 包内 `chronosync` / `chronosync.exe` 与构建产物 SHA256 一致。
- Linux 包内 `chronosync --version` = `1.1.0`。
- Windows 包 `start/stop-*.bat` 为 CRLF。
- 包内 `BUILD-INFO.txt` 包含版本、构建时间、target、CPU 变体、glibc/运行库要求、集群说明。
- `sha256sum -c SHA256SUMS` 全部 OK。
- 源码包包含源码、WebUI、文档、安装器、Windows 模板；不包含 `target/`、`dist/`、日志或发布报告。
- 包文本敏感扫描：token/真实密码/真实内网 IP = 0。

## 4. 实际安装验证

在 x86 构建测试 VM 以 root 执行：

```bash
install.sh install --no-start --cluster-mode server --cluster-id installtest \
  --cluster-secret-file ./cluster.key --port 18890 --bind 127.0.0.1
```

结果：

- `/opt/chronosync`、`run.sh`、`cluster.key`（0600）、`cluster-state`（0700）生成。
- systemd unit 生成；服务保持 `inactive`（未启动）。
- `install.sh status` 显示 `1.1.0`。
- `install.sh uninstall --purge` 后 `/opt/chronosync` 和 unit 清理。
- 未写时钟。

## 5. 已知平台限制

- Innuos（glibc 2.31）和 RoonServer（缺少匹配动态加载器）不能运行动态 1.1.0；需要源码构建或静态 musl 包。
- 静态 musl 包尚未提供。
- Linux 包要求 glibc ≥ 2.39。

## 6. 构建结论

构建、包内容、版本元数据、安装/卸载和 SHA256 通过；可以作为 1.1.0 发布资产。平台兼容性限制和未完成的广播/分区矩阵必须在发布说明中保留。
