# ChronoSync 1.3.0 分区/广播矩阵

- 日期：2026-11-06
- 脚本：`scripts/cluster_matrix.py`
- 运行：本机 loopback，多进程，独立 discovery/time 端口；不使用 auto_apply，不写时钟
- 结果：**4/4 PASS**

| 场景 | 配置 | 结果 |
| --- | --- | --- |
| 广播屏蔽/seed 单播 | A server 模式；B client；discovery 端口不同；B seed A | PASS：A server，B client，B 取得 offset |
| 双 server 脑裂 | A/B auto；discovery 端口隔离；无 seed | PASS：A/B 均为 server，term 各为 1 |
| 分区合并 | 重启 B，使用 A 的 discovery/time 端口并 seed A | PASS：A server，B client，server_id=A |
| server 失联重选 | 停止 A，B 保持 auto | PASS：B 升为 server，term=2 |

详细输出见 `reports/CLUSTER-MATRIX-1.3.0.log`（由脚本生成时可保存）。该矩阵使用端口隔离模拟广播不可达，不覆盖真实交换机/AP 隔离和跨网段路由；真实部署仍要求 seed 回退和可信 L2。
