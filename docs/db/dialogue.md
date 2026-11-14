# 对话表 `dialogue`

存储对话元数据；消息内容在 `designdb/<dialogue_id>/messages`。

| 列 | 类型 / 默认 | 说明 |
| --- | --- | --- |
| `id` | TEXT PK | 对话 UUID |
| `title` | TEXT DEFAULT '' | 标题 |
| `token_count` | INTEGER DEFAULT 0 | 预留 |
| `status` | INTEGER DEFAULT 0 | 预留 |
| `is_pinned` | INTEGER DEFAULT 0 | 是否置顶 |
| `is_archived` | INTEGER DEFAULT 0 | 是否归档 |
| `show_status` | INTEGER DEFAULT 1 | `1` 显示，`2` 不显示 |
| `system_prompt` | TEXT | 对话级 system |
| `message_count` | INTEGER DEFAULT 0 | 消息条数 |
| `last_seq` | INTEGER DEFAULT 0 | 最后消息序号 |
| `created_at` | DATETIME | 创建时间（UTC） |
| `updated_at` | DATETIME | 更新时间（UTC） |

列表默认过滤 `is_archived = 0 AND show_status != 2`，按
`is_pinned DESC, updated_at DESC` 排序。消息新增后同步 `message_count` 与
`last_seq`。

轮次记录见 [`round.md`](round.md)，普通消息见 [`messages.md`](messages.md)。
