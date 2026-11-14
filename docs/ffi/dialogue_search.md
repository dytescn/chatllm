# dialogue_search

- **导出函数**：`post_dialogue_search`
- **用途**：按标题搜索对话

## 请求

可省略 body，字段与 [`dialogue_list`](dialogue_list.md) 相同：

| 字段 | 类型 | 必填 | 说明 |
| --- | --- | --- | --- |
| `keyword` | string | 否 | 标题关键词；为空时退化为普通列表 |
| `page` | number | 否 | 页码 |
| `page_size` | number | 否 | 每页条数 |

```json
{ "keyword": "测试", "page": 1, "page_size": 20 }
```

## 响应

结构与 [`dialogue_list`](dialogue_list.md) 一致：

```json
{ "list": [], "page": 1, "page_size": 20, "total": 0 }
```

`keyword` 按 `title` 不区分大小写匹配。
