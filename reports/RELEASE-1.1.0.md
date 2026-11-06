# ChronoSync v1.1.0 发布报告

- 状态：**已发布**
- 目标仓库：`https://github.com/harmonyosnews/chronosync`
- 发布版本：**1.1.0**
- 发布 tag：`v1.1.0`
- 冻结 commit：`74ddb958dca273a948ecb735697d5a202dae7bf7`
- Release ID：`436086887`
- 发布页：`https://github.com/harmonyosnews/chronosync/releases/tag/v1.1.0`
- 构建时间：`20261106-130000`
- 发布方式：源码提交 + GitHub Release 二进制资产
- WebUI：编译时嵌入，单文件可执行

## 1. 本版重点

1. 新增 `server` / `client` / `auto` 集群模式，默认 `off`。
2. 手动 server + client 自动发现/取时；生产推荐手动 server。
3. `auto` 选举：随机退避 + `term/node_id` 确定性仲裁 + 心跳租约。
4. HMAC-SHA256 控制消息与时间响应；共享集群密钥。
5. server 外部 NTP 验证 + 后台周期刷新；参考年龄 >5 分钟停止供时。
6. client 5 次采样、中位数 offset、最低 RTT；`auto_apply` 默认关闭。
7. WebUI 集群状态徽标；`/api/cluster/state`。
8. 安装器集群参数、密钥生成/预置、服务启动参数、防火墙提示。
9. 修复 client 对当前 server 心跳未续租导致周期性误判失联的缺陷。

## 2. 范围与限制

- 首版只支持同一二层网段；跨网段使用 seed/unicast，不承诺自动发现。
- 不兼容标准 NTP 123；使用私有 NTP-like UDP 端口（默认 `18687/udp`）。
- 集群默认关闭；`auto` 和 `auto_apply` 必须显式开启。
- Linux 动态包要求 glibc ≥ 2.39。
- Innuos（glibc 2.31）、RoonServer（缺少动态加载器）不在动态包支持范围。
- 静态 musl 包未提供。
- 未完成广播阻断/分区脑裂/跨网段自动选举的完整故障矩阵。
- 生产建议手动 server；auto 仅用于可信局域网。
- 主机密钥不固定、HTTP 无 TLS、localStorage 明文密码是已知风险，见审计报告。

## 3. 验证摘要

| 项目 | 结果 |
| --- | --- |
| 单元测试 | 31 passed |
| fmt/clippy | PASS |
| JS/浏览器/安装器 | PASS |
| 实际 root 安装/卸载 | systemd 测试 VM PASS（未启动、未写时钟） |
| Linux/Windows/ARM 构建 | 1.1.0 六平台构建通过 |
| 9 节点真实矩阵 | ARM server + 8 client，10s/20s 保持同一 server；全部 `applied=false` |
| ARM 双节点 | client offset -19ms，rtt 0ms |
| ARM server + Windows client | offset 295ms，rtt 1ms；修复后 8s/22s 心跳稳定 |
| 错误密钥 | client 拒绝，0 有效样本 |
| 无有效 NTP | client 不连接、不写时钟 |
| 非 root auto_apply | 权限拒绝，不写时钟 |
| 自动端口 | 10013/权限错误回退验证通过 |
| 包/哈希 | 7 归档、二进制哈希一致、SHA256SUMS 全 OK |
| 敏感扫描 | token/真实密码/真实内网 IP = 0 |

详细见：

- [构建报告](BUILD-1.1.0.md)
- [集群测试](CLUSTER-TESTS-1.1.0.md)
- [闸门日志](gates-1.1.0.log)

## 4. 发布资产

7 个归档：

- Linux x86_64 v1/v2/v3/v4
- Linux ARM64 armv8a
- Windows x86_64 generic
- source

另附 `SHA256SUMS`、本报告、构建报告、集群测试报告。

## 5. 安装

```bash
sha256sum -c SHA256SUMS
tar -xzf chronosync-1.1.0-linux-x86_64-v1-20261106-130000.tar.gz
cd chronosync-1.1.0-linux-x86_64-v1-20261106-130000
sudo ./install.sh --auth admin:'换成强密码'
```

集群示例：

```bash
# server（生成 key 后复制给所有 client）
sudo ./install.sh --cluster-mode server --cluster-id lab

# client（必须使用同一 key）
sudo ./install.sh --cluster-mode client --cluster-id lab \
  --cluster-secret-file /path/to/cluster.key \
  --cluster-seed 192.0.2.10:18686
```

## 6. 安全说明

- 集群消息 HMAC 认证只证明“持有同一密钥”；不防密钥泄露。
- 时间写入需要 root/管理员；`auto_apply` 默认关闭。
- 集群端口只放行可信内网。
- 发布前已检查仓库/包不含 token、真实密码、真实内网 IP 清单。
- 1.1.0 tag/Release 发布后冻结；后续修改进入新版本。

## 7. 发布后回填

- 仓库：`harmonyosnews/chronosync`，公开仓库，默认分支 `main`
- tag：`v1.1.0`，annotated tag `70c472a9e8dc943c5af02f040a67ebd4eb264b31`，指向冻结 commit `74ddb958dca273a948ecb735697d5a202dae7bf7`
- Release：ID `436086887`，`https://github.com/harmonyosnews/chronosync/releases/tag/v1.1.0`
- 初传资产 8 个：6 个平台包、source 包、`SHA256SUMS`；GitHub 回读大小和 `sha256:` digest 与本地一致
- 本报告、构建报告、集群测试报告随后作为 Release 文本资产上传
- 发布后的文档回填提交只修改报告/闸门状态，不改变二进制、源包或 tag
- 技术产物以冻结 commit、Release 资产和 `SHA256SUMS` 为准
