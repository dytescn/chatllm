# chatllm 数据库

## 1. 对话表 `dialogue`

用途：存储对话元数据。消息内容保存在 `designdb/<dialogue_id>` 的
`messages` 集合中。

### 1.1 字段定义

| 列 | 类型 / 默认 | 非空 | 说明 |
| --- | --- | --- | --- |
| `id` | `TEXT PRIMARY KEY NOT NULL` | 是 | 对话 UUID（36 字符） |
| `title` | `TEXT NOT NULL DEFAULT ''` | 是 | 对话标题，默认空字符串 |
| `token_count` | `INTEGER NOT NULL DEFAULT 0` | 是 | 预留，当前未使用 |
| `status` | `INTEGER NOT NULL DEFAULT 0` | 是 | 预留 |
| `is_pinned` | `INTEGER NOT NULL DEFAULT 0` | 是 | 是否置顶：`0` 否，`1` 是 |
| `is_archived` | `INTEGER NOT NULL DEFAULT 0` | 是 | 是否归档：`0` 否，`1` 是；列表默认排除归档 |
| `show_status` | `INTEGER NOT NULL DEFAULT 1` | 是 | 显示状态：`1` 正常显示，`2` 不显示（插件对话） |
| `system_prompt` | `TEXT` | 否 | 对话级 system；例如 think_base skills 内容 |
| `message_count` | `INTEGER NOT NULL DEFAULT 0` | 是 | 消息条数 |
| `last_seq` | `INTEGER NOT NULL DEFAULT 0` | 是 | 最后消息序号；默认 `0` 表示无消息 |
| `created_at` | `DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP` | 是 | 创建时间，文本时间（UTC） |
| `updated_at` | `DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP` | 是 | 更新时间，文本时间（UTC）；写入时刷新 |

### 1.2 状态与标记约定

- `is_pinned`：`0` 未置顶，`1` 置顶。
- `is_archived`：`0` 未归档，`1` 归档；列表默认排除归档。
- `show_status`：`1` 正常显示；`2` 不显示（插件对话）。
- `status`：预留字段，当前默认 `0`。
- `token_count`：预留字段，当前默认 `0`。

### 1.3 统计字段维护约定

- `message_count`：消息条数。
- `last_seq`：最后消息序号，默认 `0` 表示无消息。
- 消息新增、删除时需同步维护上述两个字段，具体由消息表设计确定。

### 1.4 时间字段说明

- `created_at`：创建时间，文本时间（UTC）。
- `updated_at`：更新时间，文本时间（UTC）。
- 当前默认值 `CURRENT_TIMESTAMP` 仅在插入时生效；更新时需由应用层显式刷新，或通过触发器兜底。

### 1.5 默认列表查询约定

列表默认排除归档对话和 `show_status = 2` 的对话，并按置顶、更新时间排序：

```sql
WHERE is_archived = 0
  AND show_status != 2
ORDER BY is_pinned DESC, updated_at DESC