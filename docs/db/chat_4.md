# chat_4 reply 调用插件

在 `chat_3` 的基础上，`reply` 阶段增加插件调用示例：先按默认分类 `base` 检索
`chat_reply`，未命中再用 plugins 输出检索；命中 `reply_base` 并回调。

```json
{
  "_id": "c7a2f35d-1e8b-4a6c-9d70-3f5e2b8c1a94",
  "dialogue_id": "11111111-1111-4111-8111-111111111111",
  "seq": 1,
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
            "params": { "prompt": "你好" },
            "response": { "code": 200, "data": { "content": "你好" } },
            "start_time": 1793785051618,
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
        "calls": [
          {
            "step": 1,
            "start_time": 1793785051621,
            "plugins_type": "chat_plugins",
            "plugin_uuid": "a1b2c3d4-0000-4000-8000-000000000001",
            "callback": "chat",
            "params": { "prompt": "你好" },
            "response": {
              "code": 200,
              "data": { "content": "你好do_chat_test" }
            },
            "end_time": 1793785051622,
            "status": "ok"
          }
        ],
        "prompts": ["问候"],
        "outputs": ["你好do_chat_test"],
        "end_time": 1793785051622,
        "status": "ok"
      },
      "reply": {
        "start_time": 1793785051623,
        "input": { "content": "你好do_chat_test" },
        "calls": [
          {
            "step": 1,
            "plugin": "reply_base",
            "plugin_uuid": "9a7f3c2e-5b1d-4e8a-9c6f-2d4b8a1e7c30",
            "callback": "chat",
            "start_time": 1793785051623,
            "params": { "prompt": "你好do_chat_test" },
            "response": {
              "code": 200,
              "data": { "content": "你好，我是系统信息助手。" }
            },
            "end_time": 1793785051624,
            "status": "ok"
          }
        ],
        "outputs": ["你好，我是系统信息助手。"],
        "end_time": 1793785051625,
        "status": "ok"
      }
    }
  ],
  "sys": { "content": "你好，我是系统信息助手。" },
  "status": "ok",
  "created_at": 1793785051625
}
```

`reply.calls` 记录本次插件调用的 `plugin`、`plugin_uuid`、`callback`、`params`、
`response` 与开始 / 结束时间；reply 不使用 `prompts`。
