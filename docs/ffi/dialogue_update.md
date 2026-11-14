# dialogue_update

- **导出函数**：`post_dialogue_update`
- **用途**：更新对话字段

## 请求

| 字段 | 类型 | 必填 | 说明 |
| --- | --- | --- | --- |
| `uuid` | string | 是 | 对话 id；兼容 `id` |
| `title` | string | 否 | 新标题 |
| `show_status` | number | 否 | `1` 显示，`2` 不显示 |
| `is_pinned` | number | 否 | `0` 否，`1` 是 |
| `is_archived` | number | 否 | `0` 否，`1` 是 |

只更新出现的字段，并刷新 `updated_at`。

```json
{ "uuid": "<dialogue-id>", "title": "新标题", "is_pinned": 1 }
```

## 响应

```json
{ "code": 200, "data": { "id": "<dialogue-id>" }, "msg": "success" }
```
