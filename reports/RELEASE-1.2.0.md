# ChronoSync v1.2.0 发布报告

- 状态：**已发布并部署**
- 目标仓库：`https://github.com/harmonyosnews/chronosync`
- 发布版本：**1.2.0**
- 发布 tag：`v1.2.0`
- 冻结 commit：`dcc36d00471fd7cc536fcaec7c21cd0eb7728d02`
- Release ID：`436114558`
- 发布页：`https://github.com/harmonyosnews/chronosync/releases/tag/v1.2.0`
- 构建时间：`20261106-133000`
- 发布方式：源码提交 + GitHub Release 资产
- WebUI：编译时嵌入单文件二进制

## 1. 本版重点

1. Linux x86_64 v1-v4 与 ARM64 改为 **musl 静态链接**，不依赖目标 glibc。
2. x86_64 在 `.133` Debian 11 Docker 构建；ARM64 在 `.230` 原生 musl 构建。
3. 修复 static PIE：使用 `relocation-model=static + -no-pie`，产物无动态解释器。
4. Innuos（glibc 2.31）、RoonServer（无动态加载器）静态包可执行。
5. Windows generic 同步到 1.2.0；集群功能保持 1.1.0 行为。
6. 发布包命名增加 `static` 标签；BUILD-INFO 明确无 glibc 依赖。

## 2. 验证摘要

| 项目 | 结果 |
| --- | --- |
| x86 v1-v4 / ARM64 静态 | `file` statically linked；`ldd` 非动态 |
| Innuos / RoonServer | 静态包 `--version` = 1.2.0；Innuos 参与静态集群 |
| 9 节点集群矩阵 | 1.1.0 同源代码通过 |
| 1.2.0 静态集群 | ARM server + x86 + Innuos client，同一 server，Innuos offset 19ms、applied=false |
| 11 台部署 | 11/11 安装 1.2.0 并启动；1 server + 10 client，全部 applied=false |
| Windows/ARM 启动 | 1.2.0 构建/启动验证 |
| 包/哈希 | 7 归档 OK；SHA256SUMS 全 OK |
| 敏感扫描 | 0 |

## 3. 已知限制

- 集群默认 off；生产建议手动 server，auto 仅实验；auto_apply 默认关闭。
- 首版单 L2，不使用标准 NTP 123。
- 广播阻断/分区脑裂完整矩阵未完成。
- Windows 使用 MinGW，不是 musl 静态。
- 部署到验证机时默认 `--no-start`，不自动启动服务/不写时钟。

## 4. 发布资产

- Linux x86_64 v1/v2/v3/v4 static
- Linux ARM64 armv8a static
- Windows x86_64 generic
- source、SHA256SUMS、build/cluster/deployment reports
- 可打印使用手册 `ChronoSync-使用手册-v1.2.0.pdf`（17 页）

## 5. 发布后回填

- Release ID：`436114558`
- 冻结 commit：`dcc36d00471fd7cc536fcaec7c21cd0eb7728d02`
- annotated tag：`v1.2.0`
- 8 个初始资产已上传并完成 GitHub digest 回读
- 11 台验证机部署完成；详见 `reports/DEPLOYMENT-1.2.0.md`
