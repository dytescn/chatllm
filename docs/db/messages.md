# 消息文档 `messages`

存储位置：`path = <dialogue_id>`、`collection = messages`，按 `seq` 升序。
轮次记录见 [`round.md`](round.md)。

## 普通消息

| 字段 | 类型 | 说明 |
| --- | --- | --- |
| `_id` | TEXT | 消息 id，UUID v4 |
| `dialogue_id` | TEXT | 对话 uuid |
| `seq` | INTEGER | 递增序号 |
| `role` | TEXT | `user` / `assistant` / `think` / `plugins` / `reply`；轮次记录无该字段 |
| `content` | TEXT / ARRAY | 普通消息正文；阶段消息为阶段 JSON；轮次记录为阶段数组 |
| `created_at` | INTEGER | Unix 毫秒 |

```json
{
  "_id": "b7e2f0c1-9a4d-4f6e-8c21-5d3a0b7e9f10",
  "dialogue_id": "11111111-1111-4111-8111-111111111111",
  "seq": 1,
  "role": "user",
  "content": "你好",
  "created_at": 1793785051554
}
```

写入流程：读取 `dialogue.last_seq` → `seq + 1` → 插入消息 → 更新
`dialogue.last_seq`、`message_count`、`updated_at`。

## 阶段消息

每个阶段完成后，除更新轮次记录外，还会单独插入一条阶段消息：

- `role = "think"`：think 阶段结果；
- `role = "plugins"`：plugins 阶段结果；
- `role = "reply"`：reply 阶段结果。

`content` 为该阶段的 JSON 对象，与轮次记录 `content` 数组中的对应元素一致。

## 读取视图

`chat_detail` 读取时做字段映射：

| 输出字段 | 来源 | 说明 |
| --- | --- | --- |
| `uuid` | `_id` | 移除 `_id` |
| `id` | `seq` | 新增，与 `seq` 相同 |
| `seq` | `seq` | 保留 |
| `dialogue_id` | 原字段 | 对话 uuid |
| `role` | 原字段 | `user` / `assistant` |
| `content` | — | 列表不返回（轮次记录 / 阶段消息的 JSON 可能很大） |
| `content_html` | 固定空字符串 | markparse 已移除 |
| `created_at` | 原字段 | Unix 毫秒 |

列表视图会移除 `content` 字段，避免轮次记录与阶段消息的 JSON 撑大响应。

`chat_detail` 的 `messages.list` 同时返回普通消息、阶段消息与轮次记录；轮次记录
没有 `role`，通过 `user` / `sys` 字段识别，`content` 为 `think / plugins / reply`
数组，阶段消息的 `role` 即为对应阶段名。

`message_history`（供应商上下文）只返回 `user` / `assistant` 消息：普通消息
按 `role` 取出，轮次记录由 `user` / `sys` 还原；不附加 `content_html`。
