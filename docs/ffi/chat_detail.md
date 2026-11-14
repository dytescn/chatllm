# chat_detail

- **导出函数**：`post_chat_detail`
- **用途**：读取对话与消息，支持游标分页

## 请求

| 字段 | 类型 | 必填 | 说明 |
| --- | --- | --- | --- |
| `uuid` | string | 是 | 对话 id |
| `before_id` | number | 否 | 读取 `id < before_id` 的更早消息 |
| `after_id` | number | 否 | 读取 `id > after_id` 的更新消息 |
| `limit` | number | 否 | 条数上限，默认 `20`，最大 `100` |

`before_id` / `after_id` 二选一；都不传时返回最新 `limit` 条。

```json
{ "uuid": "<dialogue-id>", "limit": 20 }
```

## 响应

```json
{
  "code": 200,
  "data": {
    "dialogue": {},
    "messages": {
      "list": [],
      "limit": 20,
      "first_id": null,
      "last_id": null,
      "has_more_before": false,
      "has_more_after": false
    }
  },
  "msg": "success"
}
```

消息字段与 `content_html` 说明见 [`../db/messages.md`](../db/messages.md)。
