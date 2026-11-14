# chat_2 只调用 think_base

`think` 阶段调用 `think_base`；`plugins` 不执行；`reply` 原样输出 think 结果。

```json
{
  "_id": "2f8c6a41-7d3e-4b9a-8c5d-1e0f2b7a9c34",
  "dialogue_id": "11111111-1111-4111-8111-111111111111",
  "seq": 1,
  "role": "round",
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
        "calls": [],
        "prompts": [],
        "outputs": [],
        "end_time": 1793785051621,
        "status": "skipped"
      },
      "reply": {
        "start_time": 1793785051621,
        "input": { "content": "你好" },
        "calls": [],
        "outputs": ["你好"],
        "end_time": 1793785051623
      }
    }
  ],
  "status": "ok",
  "created_at": 1793785051624
}
```

`think` 的 `calls` 记录 `think_base` 调用及其开始 / 结束时间；`plugins` 为空对象；
`reply` 没有任何插件调用，直接输出 think 的结果。
