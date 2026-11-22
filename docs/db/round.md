# 轮次记录

一次 `chat` 调用写入一条轮次记录，记录 `user`（用户输入）、`content`
（`think` / `plugins` / `reply` 数组）与 `sys`（系统回答）。轮次记录没有
`role` 字段。

记录在 think 阶段完成后立即创建（此时写入 `user`），plugins / reply 阶段
完成后补齐 `content` 与 `sys`。消息列表按 `seq` 升序返回时，轮次记录与
assistant 消息可能交错；轮次记录通过 `user` / `sys` 字段识别。

## 字段

| 字段 | 类型 | 说明 |
| --- | --- | --- |
| `_id` | TEXT | 记录 id，UUID v4 |
| `dialogue_id` | TEXT | 对话 uuid |
| `seq` | INTEGER | 递增序号 |
| `user` | OBJECT | `{ content }`，本轮用户输入 |
| `content` | ARRAY | `[{ think, plugins, reply }]` |
| `sys` | OBJECT | `{ content }`，系统回答（reply 完成后写入） |
| `status` | TEXT | `ok` / `skipped` / `error` |
| `created_at` | INTEGER | Unix 毫秒 |

阶段对象：

| 字段 | 说明 |
| --- | --- |
| `method` | 调用方法（可选） |
| `start_time` | 阶段开始时间（Unix 毫秒） |
| `input` | 阶段输入 |
| `prompts` | think 的提示词数组；chat_plugins 与 reply 不再返回 |
| `inputs` | chat_plugins 的输入文本数组（每条对应一个 prompt） |
| `outputs` | 阶段输出数组 |
| `end_time` | 阶段结束时间（Unix 毫秒） |
| `precalls` | 前置回调（可选） |
| `calls` | 插件调用 |
| `status` | `ok` / `skipped` / `error` |

`calls` 元素：

| 字段 | 说明 |
| --- | --- |
| `step` | 调用序号 |
| `plugin` / `plugins_type` | 插件名或插件类型 |
| `plugin_uuid` | 插件 uuid |
| `taskflow` | taskflow 元素（`uid` / `name`），可选 |
| `callback` | 回调方法，实际调用 `post_<callback>` |
| `start_time` | 调用开始时间（Unix 毫秒） |
| `params` | 传给回调方法的完整参数，通常含 `prompt` |
| `response` | 回调返回内容 |
| `end_time` | 调用结束时间（Unix 毫秒） |
| `status` / `error` | 状态与失败原因 |

`prompts` 统一放在阶段对象内，`calls` 不再单独记录 `prompt`。

## 示例

```json
{
  "_id": "5e1c7a90-2b4d-4f8a-9c36-7d0b2e5a1f83",
  "dialogue_id": "11111111-1111-4111-8111-111111111111",
  "seq": 3,
  "user": { "content": "你好" },
  "content": [
    {
      "think": {
        "start_time": 1793785051618,
        "input": { "content": "你好" },
        "calls": [
          {
            "step": 1,
            "plugin": "think_base",
            "plugin_uuid": "9a7f3c2e-5b1d-4e8a-9c6f-2d4b8a1e7c30",
            "callback": "chat",
            "start_time": 1793785051618,
            "params": { "prompt": "你好" },
            "response": { "code": 200, "data": { "content": "你好" } },
            "end_time": 1793785051620,
            "status": "ok"
          }
        ],
        "prompts": ["问候"],
        "outputs": ["你好"],
        "end_time": 1793785051620,
        "status": "ok"
      },
      "plugins": {
        "start_time": 1793785051621,
        "inputs": ["你好"],
        "calls": [
          {
            "step": 1,
            "start_time": 1793785051621,
            "plugin": "do_test_chat",
            "plugin_uuid": "a1b2c3d4-0000-4000-8000-000000000001",
            "prompt": "问候",
            "params": { "prompt": "你好", "keyword": "问候" },
            "callbacks": ["chat"],
            "response": "你好do_chat_test",
            "end_time": 1793785051622,
            "status": "ok"
          }
        ],
        "outputs": ["你好do_chat_test"],
        "end_time": 1793785051622,
        "status": "ok"
      },
      "reply": {
        "start_time": 1793785051623,
        "input": { "content": "你好do_chat_test" },
        "outputs": ["你好do_chat_test"],
        "end_time": 1793785051624,
        "status": "ok"
      }
    }
  ],
  "sys": { "content": "你好do_chat_test" },
  "status": "ok",
  "created_at": 1793785051624
}
```

## 写入约束

- 一次 `chat` 调用一条轮次记录（无 `role` 字段）；
- `content` 元素按 `think` / `plugins` / `reply` 顺序；
- 每次插件调用（成功 / 失败 / 跳过）写入 `calls`；
- `prompts` 放在阶段对象内，`params` 记录回调方法完整入参；
- 阶段与插件调用运行时记录 `start_time` / `end_time`（Unix 毫秒）；
- 阶段内容同时以独立消息写入（`role = think / plugins / reply`）；
- 普通消息、阶段消息与轮次记录共用 `dialogue.last_seq`，写入后同步 `message_count`；
- `message_history` 从轮次记录的 `user` / `sys` 还原 user / assistant 上下文。
