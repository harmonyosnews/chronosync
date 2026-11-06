# ChronoSync v1.4.0 发布报告

- 状态：**已构建、11 台验证机部署与真机只读验证完成；待冻结提交/Release 回读**
- 目标仓库：`https://github.com/harmonyosnews/chronosync`
- 版本：**1.4.0**
- 目标 tag：`v1.4.0`
- 构建时间：`20261106-150000`
- 冻结 commit / Release ID：待回填
- 发布页：待回填

## 1. 本版重点

1. WebUI 集群控制条：NTPv4 热开关、手动指定本机为 Server、程序重启。
2. NTPv4 运行中绑定/释放 UDP 123；绑定失败保留错误，私有时间服务继续。
3. 手动 Server 以 term+1 接管；取消后按启动模式回退；固定 Server 在现 Server 失联后重新接管。
4. 运行设置 `runtime.json`（日志目录，0600）持久化；修改失败回滚。
5. 设备清单“偏差”列：检测显示当前偏差，成功写入后显示带符号残余偏差。
6. 安装器默认/文档统一为 `--bind 0.0.0.0`；同步补齐 1200–1366px 表格布局。
7. 修复手动接管瞬间旧 Server 采样竞态：角色/Server 不匹配时不再写入过期 `last_error`。

## 2. 验证摘要

| 项目 | 结果 |
| --- | --- |
| fmt / clippy / tests | PASS；39 tests |
| WebUI JS / 浏览器 | PASS；桌面/移动/7 档宽度，console/page error 0 |
| 六平台构建 | x86 v1–v4、ARM64、Windows 全部产出 |
| Linux 静态 | `statically linked`，无 `INTERP`；x86 v1–v4 冒烟 200 |
| ARM64 静态 | `.230` 原生构建；`/api/health` 200 |
| 安装/升级 | 11/11 按“卸载→重新安装”；全部 `BIND=0.0.0.0` |
| 真机只读 | 1 server + 10 client；server_id 一致；`last_error=null`；`applied=false` |
| NTPv4 热开关 | 48B、LI0/VN4/mode4/stratum3；关闭超时；重开恢复 |
| 程序重启 | HTTP 202；instance_id 变化；版本保持 1.4.0 |
| 手动 Server | term 4 接管，term 5 回落；竞态修复后 `last_error=null` |
| 敏感扫描 | token/私钥/真实内网 IPv4/AWS key = 0 |

详细见 [构建报告](BUILD-1.4.0.md)、[部署记录](DEPLOYMENT-1.4.0.md)、[WebUI 控制验证](WEBUI-CONTROLS-1.4.0.md)、[闸门日志](gates-1.4.0.log)。

## 3. 发布资产

7 个归档：

- Linux x86_64 v1/v2/v3/v4 static
- Linux ARM64 armv8a static
- Windows x86_64 generic
- source

另附 `SHA256SUMS`、本报告、构建报告、部署记录、WebUI 控制验证、使用手册 PDF、闸门日志。

## 4. 安装

```bash
sha256sum -c SHA256SUMS
tar -xzf chronosync-1.4.0-linux-x86_64-v1-static-20261106-150000.tar.gz
cd chronosync-1.4.0-linux-x86_64-v1-static-20261106-150000
sudo ./install.sh --bind 0.0.0.0 --auth admin:'换成强密码'
```

集群升级示例（**先卸载、保留 key/node-id，再重装**）：

```bash
# 记录并备份 install.conf / cluster.key / cluster-state/node-id
sudo ./install.sh uninstall
sudo ./install.sh install --no-start --bind 0.0.0.0 \
  --cluster-mode client --cluster-id lab \
  --cluster-secret-file /path/to/cluster.key \
  --cluster-seed 192.0.2.10:18686
# 恢复 node-id，启动服务，检查 /api/health 与 /api/cluster/state
```

## 5. 安全与限制

- 标准 NTPv4 无 HMAC；默认关闭，只在可信内网开启。
- WebUI 重启仅 Unix；Windows 返回 501，需使用启动/停止脚本或服务管理器。
- Windows 包完成交叉编译/PE 校验，未在真实 Windows 运行。
- 手动 Server 接管会造成客户端短暂取时中断；不要在正常运行时同时指定多个 Server。
- 未执行任何时间写入；`auto_apply=false`。

## 6. 发布后回填

- 冻结 commit / annotated tag / Release ID / URL：待填。
- 14 个资产 digest 回读：待填。
- 11 台验证机已部署 1.4.0；发布后核对 Release 包与已部署二进制哈希一致。
