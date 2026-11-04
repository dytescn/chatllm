# reply 阶段

`reply` 接收 `plugins` 的合并结果，按结果检索 `plugins_reply`，调用命中的插件
生成最终回复；未命中或调用失败时回退到 `reply_base` 清洗。实现见
`src/utils/reply.rs`。

## 输入

```text
result = plugins 合并结果
prompt = 用户原始输入
```

调用：

```rust
Reply::from_result(result, prompt)
```

## 检索

1. 用 `result` 作为关键词检索 `plugins_reply`，`top_k = 5`；
2. 取 Top1；
3. 解析字段：

| 字段 | 来源 | 用途 |
| --- | --- | --- |
| `plugin` | `meta.plugin` / `value.lib_name` / `value.plugin` | 供应商或业务插件 |
| `plugins_uid` | `meta.plugins_uid` / `meta.plugin_uuid` / `value.*` | 对话 id |
| `plugin_name` | `meta.plugin_name` / `value.plugin_name` / `value.name` | 读取 skills |
| `ffi_symbol` | `meta.ffi_symbol` / `value.ffi_symbol` | 默认 `chat` |

示例：

```json
{
  "key": "reply-base",
  "value": "{\"lib_name\":\"deepseek\",\"plugin_uuid\":\"<reply dialogue uuid>\",\"plugin_name\":\"reply_base\"}",
  "text": "回复 总结 结果 输出",
  "meta": {
    "plugin": "deepseek",
    "ffi_symbol": "chat",
    "plugins_uid": "<reply dialogue uuid>",
    "plugin_name": "reply_base"
  }
}
```

## 对话

- `plugins_uid` 是合法 uuid：
  - 对话已存在：直接使用；
  - 不存在：创建 `show_status = 2` 的对话，id 使用该 uuid；
  - `system_prompt` 来自 `./plugins/<plugin_name>/<plugin_name>.md`。
- 没有 `plugins_uid`：创建新的 reply 对话。
- 写入 user 消息（plugins 结果）和 assistant 消息（最终回复）。

## 插件执行

### 供应商插件

`plugin` 为 `deepseek` / `libdeepseek` / `siliconflow_t2t` /
`libsiliconflow_t2t` 时，直接调用：

```text
cloud::chat_with(supplier, result, messages)
```

`messages` 包含：

1. `plugin_name` skills（存在时）；
2. 该对话的历史消息；
3. 当前 plugins 结果。

### 业务插件

其他插件按 `post_<ffi_symbol>` 调用，请求：

```json
{ "prompt": "<plugins 结果>" }
```

取响应 `data.content`。

## 回退

以下情况回退到 `reply_base` 清洗：

- `plugins_reply` 未命中；
- 插件调用失败；
- 插件返回空内容；
- `plugins_reply` 集合不可用。

`reply_base` 清洗规则与资源包见
[`think.md`](think.md) 与 [`reply_base`](../docs/ffi.md)。

## 测试

```bash
cargo test --lib
cargo test --test utils_reply
```

覆盖：

- `plugins_reply` 记录解析；
- `plugins_uid` / `plugin_uuid` 兼容；
- 未命中时回退 reply_base；
- `Reply` 的 HTML 生成。
