# dialogue_list

- **导出函数**：`post_dialogue_list`
- **用途**：对话分页列表

## 请求

可省略 body：

| 字段 | 类型 | 必填 | 说明 |
| --- | --- | --- | --- |
| `keyword` | string | 否 | 非空时按标题搜索 |
| `page` | number | 否 | 页码，默认 `1` |
| `page_size` | number | 否 | 每页条数，`1–100`，默认 `20` |

```json
{ "keyword": "标题", "page": 1, "page_size": 20 }
```

## 响应

```json
{ "list": [], "page": 1, "page_size": 20, "total": 0 }
```

默认过滤 `is_archived = 0 AND show_status != 2`，按
`is_pinned DESC, updated_at DESC` 排序。
