# ChronoSync 1.2.0 验证机部署记录

| 项目 | 内容 |
| --- | --- |
| 日期 | 2026-11-06 |
| 版本 | 1.2.0（静态 musl / Windows 包） |
| 范围 | 11 台可达验证机；`.205` 不可达，剔除 |
| 方式 | 静态包上传 + `install.sh install --no-start`，随后启动服务 |
| 时钟策略 | `auto_apply=false`；未执行时间写入 |
| 部署后状态 | 11/11 服务运行；1 server + 10 client；全部 applied=false |
| 集群 | `.230` 手动 server；其余 10 台 client，seed `.230:18686` |
| 密钥 | 部署密钥仅存在目标 `/opt/chronosync/cluster.key`（0600），未写入仓库 |

## 1. 安装结果

11/11 主机完成安装，`/opt/chronosync/chronosync --version` 均为 `1.2.0`：

- x86 构建机、Euphony、Innuos、2 台 GentooPlayer、RoonServer、4 台 Diretta：x86_64 静态包。
- ARM 构建机、Diretta aarch64：ARM64 静态包。
- `--no-start` 安装；原有 `.230`（1.1.0）和 Gentoo .227（1.0.0）的旧服务先停止后升级。
- AudioSys 目标安装到 `/opt/chronosync` + `/audiosys/chronosync`，CONFIG 钩子需现场确认。

## 2. 启动与集群结果

启动全部 11 台后：

- server：ARM 构建机，`role=server`、`ntp_verified=true`、`peers=10`。
- client：其余 10 台，全部 `server_id` 指向同一 server，`last_error=null`。
- 全部 `auto_apply=false`、`applied=false`，未写目标时钟。
- client offset 约 -300ms 到 +405ms（设备本地时钟差）；RTT 0–2ms。
- 各服务已保持运行；WebUI 仅绑定 `127.0.0.1:8686`，集群 UDP 使用 `18686/18687`。
- `.205` 未部署（TCP 22 超时），不计入 11 台成功数。

## 3. 限制

- 自动写入未开启；需要显式改 `auto_apply` 并重启服务，且需维护窗口。
- 集群为单 L2 + seed 模式；广播阻断/分区脑裂未做完整矩阵。
- AudioSys 的持久钩子是否在 CONFIG 分区生效需重启验证；本轮未重启设备。
- 未在真实生产时间上执行写入；这是只读/展示部署。
