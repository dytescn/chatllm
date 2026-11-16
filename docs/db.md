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
