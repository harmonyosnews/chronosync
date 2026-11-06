# AGENTS.md — ChronoSync 代理操作规则

本文件约束在本仓库工作的自动化代理。它不替代用户指令；冲突时以用户当前指令为准，但安全/发布边界必须先说明。

## 1. 项目坐标

- 仓库根：`/home/kele/harmonyosnews/chronosync`
- 产品版本：`1.3.0`
- 发布 tag：`v1.3.0`（发布后以 tag 指向的冻结 commit 为准）
- 发布页：<https://github.com/harmonyosnews/chronosync/releases/tag/v1.3.0>
- 技术栈：Rust 2024 / Tokio / Axum / russh / 单文件 WebUI
- 文档基线：2026-11-06；24 小时长稳测试已取消

## 2. 先读什么

1. [docs/README.md](docs/README.md) — 文档地图和单一事实源。
2. [docs/纪律与闸门.md](docs/纪律与闸门.md) — G1–G14 定义。
3. [docs/开发手册.md](docs/开发手册.md) — 模块/约定。
4. [docs/程序接口.md](docs/程序接口.md) — API 契约。
5. [工程纪律.md](工程纪律.md) — 提交/安全/发布规则。
6. [docs/审计报告.md](docs/审计报告.md) / [docs/Issues整理.md](docs/Issues整理.md) — 已知风险/债务。

## 3. 硬约束

- **不伪造测试**：未执行写 `NOT RUN`；取消写 `CANCELED`；不能用“之前跑过”代替当前版本。
- **不泄露凭据**：密码/token/私钥不得出现在命令参数、日志、截图、文档、Issue、提交、Release。
- **不写入生产时钟**：默认 `dry_run=true`；真实写入需用户明确授权和维护窗口。
- **不改已发布资产**：`v1.3.0` tag/Release 不可改写；运行时/UI/安装器变更必须新版本。
- **静态支持已实现**：Linux x86_64 v1-v4、ARM64 为 musl 静态；Innuos/RoonServer 已用静态包验证。
- **不恢复 24h 测试**：该项已取消；除非用户明确恢复并另立计划。
- **不提交真实内网清单**：示例用 RFC 5737 地址（192.0.2.0/24 等）。

## 4. 工作流程

```text
理解范围 → 报告/计划 → 最小修改 → G1–G6 → 视需要 G7–G9/G12
→ G10/G11/G14 → 提交/发布（仅授权）→ 回填证据
```

- 纯文档：G1/G4/G10/G11/G14；不重建二进制。
- 运行时：新版本全量 G1–G13；构建/真机/包/Release。
- 每次修改同步文档、CHANGELOG、Issues（如涉及）。
- 不新增依赖，除非有明确收益和跨平台/许可证评估。

## 5. 常用命令

```bash
cd /home/kele/harmonyosnews/chronosync
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
sh -n install.sh
./install.sh --dry-run --binary /bin/true --no-start

# WebUI JS 语法
python3 - <<'PY'
import re
html=open('web/index.html',encoding='utf-8').read()
open('/tmp/chronosync-ui.js','w').write('\n'.join(re.findall(r'<script>(.*?)</script>',html,re.S)))
PY
node --check /tmp/chronosync-ui.js
```

构建/静态编译见 [docs/程序编译环境的搭建与实施-静态编译.md](docs/程序编译环境的搭建与实施-静态编译.md)。

## 6. 代码修改规则

- 匹配现有风格；运行时错误用 `Result`，不用 `unwrap`。
- 时间：内部 UTC epoch ms，展示北京时间；`_ms`/`_s` 后缀。
- 外部输入：长度/字符/重复校验；错误信息给出下一步，不泄露凭据。
- 可选步骤（RTC/时区）失败只警告，不冒充系统时间成功。
- 并发：保持 `activity` 互斥、8 主机并发、扫描上限。
- 日志：先 `sanitize_text`，8000 条上限；不记录密码。
- 修改 API 必须更新 `docs/程序接口.md`；修改行为更新 CHANGELOG。

## 7. 已发布边界

- 1.3.0 的 Linux 静态包、Windows 包、`SHA256SUMS` 和 Release 资产是冻结证据；1.0.2/1.1.0/1.2.0 资产保留为历史。
- 文档提交可以修正描述，但不能改变已发布二进制或包哈希。
- 若用户要求修复运行时缺陷：先确认是否发新版本；不要直接改 tag 后重建同名资产。
- 代码去重候选（Issues ISS-007–010）留到新版本，不在文档任务中顺手改。

## 8. 安全与发布

- 默认 `0.0.0.0:8686` 无 TLS；代理应提醒 `--auth`/回环/内网。
- 主机密钥不固定、localStorage 明文密码、NTP 未认证是已接受风险；不要写成已修复。
- 发布 token 只从权限文件读入内存；临时 askpass/脚本用后删除；不在命令/日志打印。
- 发布前做 G10 敏感扫描；发布后做 G13 回读并记录 SHA256 digest。
- 发现泄露：停止、撤销/改密、清理、重跑扫描、报告。

## 9. 何时停止并询问

- 需要在生产设备执行写入或改变网络/服务。
- 需要改写已发布 tag/Release 资产。
- 需要新增敏感凭据、外部服务或公开数据。
- 范围/验收标准冲突，或无法在不伪造证据的情况下完成。
- 需要恢复已取消的 24h 测试或静态发布。

## 10. 文档维护

- 新文档登记到 `docs/README.md`，不复制 API/闸门/排期全表。
- 状态必须与证据一致；链接必须可解析。
- 根目录只保留 README/CHANGELOG/AGENTS/使用手册/项目管理/工程纪律。
- 术语/单位归一化；示例使用文档地址；不出现真实密码/内网清单。
