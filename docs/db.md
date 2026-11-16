# chatllm 数据库

| 数据 | 位置 | 文档 |
| --- | --- | --- |
| 对话元数据 | `database/dialogue.db` → `dialogue` | [`db/dialogue.md`](db/dialogue.md) |
| 普通消息 | `designdb/<dialogue_id>` → `messages` | [`db/messages.md`](db/messages.md) |
| 轮次记录 | `designdb/<dialogue_id>` → `messages` | [`db/round.md`](db/round.md) |

一次 `chat` 调用写入 user / assistant 消息，以及一条 `role = "round"` 的轮次记录。
普通消息与轮次记录共用 `dialogue.last_seq`，写入后同步 `message_count`。

轮次记录包含 `think` / `plugins` / `reply` 三个阶段（`chat_plugins` 阶段写入
`plugins` 字段），插件调用记录在各阶段的 `calls`；字段与示例见
[`db/round.md`](db/round.md)。

## 外部库接口

版本以 `dytes.json` 的 `dependencies-sys` 为准：

| 依赖 | 版本 | 导出符号 | chatllm 封装 |
| --- | --- | --- | --- |
| `database` | 1.0.6 | `db_create` / `db_insert` / `db_update` / `db_delete` / `db_query` | `utils::db`：`db_create` / `db_execute` / `db_update` / `db_delete` / `db_query` |
| `designdb` | 1.0.4 | `post_db_create` / `post_db_insert` / `post_db_update` / `post_db_delete` / `post_db_query` | `utils::designdb`：`create` / `insert` / `update` / `delete` / `query` / `query_all` |
| `kv_lib` | 0.1.5 | `kv_collection` / `kv_embed` / `kv_record` / `kv_search` | `utils::kv`，见 [`kv_lib.md`](kv_lib.md) |
| `plugin_lib` | 1.1.0 | 22 个 `get_*` / `post_*` / `patch_*` / `update_*` / `delete_*` 符号 | `utils::plugins_lib`，见 [`plugins_lib.md`](plugins_lib.md) |

输出缓冲约定：`database` 至少 1 MiB；`designdb` 为 16 MiB；`kv_search` 为
1 MiB，其余 kv 接口为 64 KiB。
