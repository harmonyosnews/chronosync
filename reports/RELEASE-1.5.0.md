# ChronoSync v1.5.0 发布报告

- 状态：**已构建、11 台验证机部署与真机只读验证完成；待冻结提交/Release 回读**
- 目标仓库：`https://github.com/harmonyosnews/chronosync`
- 版本：**1.5.0**
- 目标 tag：`v1.5.0`
- 构建时间：`20261106-170000`
- 冻结 commit / Release ID / 发布页：待回填

## 1. 本版重点

1. WebUI 集群控制新增“自动写入本机时间”开关，只对 client 生效。
2. `auto_apply` 运行时热切换并持久化到 `runtime.json`（0600），重启后保持。
3. 复用既有保护：`max_offset_ms`（默认 100ms）、`max_step_ms`（默认 5000ms）、大步长拒绝/允许。
4. 控制条显示阈值/步长/大步长策略；开启前必须确认。
5. 未新增绕过阈值/步长的参数；默认仍关闭。

## 2. 验证摘要

| 项目 | 结果 |
| --- | --- |
| fmt / clippy / tests | PASS；41 tests |
| WebUI JS / 浏览器 | PASS；auto_apply 确认/开/关；7 档宽度；console/page error 0 |
| 六平台构建 | x86 v1–v4、ARM64、Windows 全部产出 |
| Linux 静态 | `statically linked`，无 `INTERP` |
| Windows | health 200；auto-port 18889→18890 |
| 11/11 部署 | 卸载→重新安装；全部 1.5.0、BIND 0.0.0.0 |
| 集群 | 1 server + 10 client；server_id 一致；last_error=null；applied=false |
| auto_apply | 全部 false；真机 true→false 切换与 runtime 持久化通过；未写时钟 |
| 阈值内集成 | 偏差 768ms、阈值 3600000ms：applied=false |
| NTPv4/重启 | 48B/LI0/VN4/mode4/stratum3；instance_id 变化 |

详细见 [构建报告](BUILD-1.5.0.md)、[部署记录](DEPLOYMENT-1.5.0.md)、[自动写入验证](AUTO-APPLY-1.5.0.md)、[闸门日志](gates-1.5.0.log)。

## 3. 发布资产

7 个归档：

- Linux x86_64 v1/v2/v3/v4 static
- Linux ARM64 armv8a static
- Windows x86_64 generic
- source

另附 `SHA256SUMS`、本报告、构建报告、部署记录、自动写入验证、闸门日志、使用手册 PDF。

## 4. 升级与安全

- 默认 `auto_apply=false`，部署不主动写本机时钟。
- 开启自动写入前必须在维护窗口确认 offset；超过阈值且不超过最大步长才会写入，大步长默认拒绝。
- 自动写入只影响 client 本机，不影响设备清单中的 SSH 目标。
- 标准 NTPv4 无 HMAC；默认关闭，只在可信内网开启。
- WebUI 重启仅 Unix；Windows 使用启动/停止脚本。

## 5. 发布后回填

- 冻结 commit / annotated tag / Release ID / URL：待填。
- 14 个资产 digest 回读：待填。
- 11 台验证机已部署 1.5.0；发布后核对 Release 包与已部署二进制哈希一致。
