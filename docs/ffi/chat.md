# chat

- **导出函数**：`post_chat`
- **用途**：新建 / 续聊对话，执行 `think → chat_plugins → reply`

## 请求

| 字段 | 类型 | 必填 | 说明 |
| --- | --- | --- | --- |
| `prompt` | string | 是 | 用户消息 |
| `uuid` | string | 否 | 续聊已有对话 |
| `show_status` | number | 否 | `1` 显示，`2` 不显示 |
| `title` | string | 否 | 标题；缺省取 `prompt` 前 20 字符 |
| `max_tokens` | number | 否 | 原样回显 |

目标对话优先级：`uuid` > 自动生成。

```json
{ "prompt": "你好", "title": "新对话" }
```

## 流程

1. `think`：用固定关键词 `init` 检索 `chat_think`（100% 匹配），命中插件并回调，结果直接交给 plugins；
2. `plugins`：用 think 的 Top1 `value` 检索 `chat_plugins`，按 `plugin_uuid` 读取 taskflow，执行回调；
3. `reply`：合并结果检索 `chat_reply`，命中插件并回调；未命中原样输出合并结果。

`check` 已移除，后续以插件形式提供。

## 响应

| 字段 | 类型 | 说明 |
| --- | --- | --- |
| `uuid` | string | 对话 id |
| `created` | boolean | 是否新建 |
| `prompt` | string | 请求回显 |
| `max_tokens` | number \| null | 请求回显 |
| `route` | string \| null | 正常为 `null` |
| `plugin_uuid` | string \| null | think 命中资源包 uuid |

```json
{
  "code": 200,
  "data": {
    "uuid": "11111111-1111-4111-8111-111111111111",
    "created": true,
    "prompt": "你好",
    "max_tokens": null,
    "route": null,
    "plugin_uuid": null
  },
  "msg": "success"
}
```

轮次记录见 [`../db/round.md`](../db/round.md)，消息字段见
[`../db/messages.md`](../db/messages.md)。
