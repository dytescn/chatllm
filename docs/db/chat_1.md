# chat_1 基础对话

最基础的轮次记录：没有 `calls`、`params`、`method` 等调用信息，只有阶段输入、
输出与运行时间。

```json
{
  "_id": "5e1c7a90-2b4d-4f8a-9c36-7d0b2e5a1f83",
  "dialogue_id": "11111111-1111-4111-8111-111111111111",
  "seq": 1,
  "user": { "content": "你好" },
  "content": [
    {
      "think": {
        "start_time": 1793785051618,
        "input": { "content": "你好" },
        "calls": [],
        "prompts": [],
        "outputs": ["你好"],
        "end_time": 1793785051620
      },
      "plugins": {
        "start_time": 1793785051618,
        "calls": [],
        "prompts": ["系统为空"],
        "outputs": [],
        "end_time": 1793785051620,
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
  "sys": { "content": "你好" },
  "status": "ok",
  "created_at": 1793785051624
}
```

`plugins` 为空对象表示没有调用任何插件；`think` / `reply` 的输入与输出都原样保留
用户内容，没有经过加工处理。`start_time` / `end_time` 为 Unix 毫秒。
