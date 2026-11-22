# chat_3 plugins 调用 taskflow

`think` 调用 `think_base`；`plugins` 直接使用 think 输出的 prompts 逐条检索 `chat_plugins`
（只取 Top1），读取 taskflow 并执行回调，只输出 outputs；`reply` 先按默认分类
`base` 检索 `chat_reply`，未命中时原样输出 plugins 结果。

```json
{
  "_id": "9b4d1e72-6c3a-4f8d-a5e2-7c1b0d3e9f46",
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
        "calls": [],
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

`plugins.calls[0].callback` 为实际调用的插件方法；`think` / `plugins` / `reply`
都记录 `start_time` / `end_time`。
