# plugins 阶段

`think` 按 `|` 拆分出的每一段，都会在 `plugins` 阶段独立检索并执行插件。
实现见 `src/utils/plugins.rs`。

## 输入与输出

```text
输入：think 拆分后的查询词数组 Vec<&str>
输出：每段的结果文本 Vec<String>
      → 空行合并后交给 reply
```

## 检索

每一段查询执行：

1. 在 `chat_plugin` 集合中检索，`top_k = 5`；
2. 取 Top1；
3. 从命中记录解析插件：
   - 优先 `meta.plugin` + `meta.ffi_symbol`；
   - 否则解析 `value` 中的 `lib_name` + `ffi_symbol`；
   - `ffi_symbol` 缺省为 `chat`。

示例记录：

```json
{
  "key": "chat-plugin-do-test-chat",
  "value": "{\"name\":\"do_test_chat\",\"lib_name\":\"do_test_chat\",\"ffi_symbol\":\"chat\"}",
  "text": "do_test_chat 测试插件",
  "meta": {
    "plugin": "do_test_chat",
    "category": "chat"
  }
}
```

未命中记录时返回：

```text
未找到相关对话内容：<查询词>
```

## 执行

### 供应商插件

插件名为 `deepseek` / `libdeepseek` / `siliconflow_t2t` /
`libsiliconflow_t2t` 时，统一走：

```text
cloud::chat_with(supplier, query, &[])
```

不加载插件动态库。

### 业务插件

按以下顺序探测动态库：

```text
./plugins/<plugin>/<plugin>.<ext>
./plugins/<plugin>/lib<plugin>.<ext>
./libary/<plugin>.<ext>
./libary/lib<plugin>.<ext>
```

调用入口：

```text
post_<ffi_symbol>
```

请求：

```json
{ "prompt": "<查询词>" }
```

响应：

```json
{ "code": 0, "data": { "content": "..." } }
```

`code` 允许 `0` 或 `200`；取 `data.content` 作为该段结果。

插件库按插件名缓存在进程内（`./plugins/...` 或 `./libary/...`），加载后不主动
卸载；`dlopen` 不持缓存锁，避免 loader lock 与其他线程的锁交叉等待。
同一线程内禁止插件重入：插件回调 chatllm 再次触发插件调用时直接返回错误
（`call_chat` 为 `400304`）。

## call_chat 直接调用

`call_chat` 复用同一套 `plugin_library` / `invoke_plugin`，但不经过 think 与
reply：

```text
输入：{ plugins_uid?, category, prompts?, context? }
  │
  ├─ kv_search(collection = category, query = prompts, top_k = 5)
  ├─ 取 Top1，解析插件与 ffi_symbol
  ├─ 插件请求：{ plugins_uid, category, prompts, context }（原样透传）
  └─ 返回插件响应 data
```

未命中插件、命中记录无插件信息或插件调用失败返回 `400304`。实现见
`src/utils/call_flow.rs`，协议见 [`../docs/ffi.md`](../docs/ffi.md) 2.6。

## 多段处理

`handle_many` 逐段执行，单段失败不阻断其他段：

- 成功：保留 `content`；
- 失败：保留 `插件处理失败：<原因>`；
- 最后 `join_results` 去掉空结果，用空行连接。

示例：

```text
问候|系统信息
  → 查询「问候」→ do_test_chat → 问候do_chat_test<time>
  → 查询「系统信息」→ do_test_chat → 系统信息do_chat_test<time>
  → 合并：问候do_chat_test<time>\n\n系统信息do_chat_test<time>
```

## 测试

```bash
cargo test --test utils_plugins
cargo test --test base_call_chat   # 需要 testenv 的 kv_lib 与 do_test_chat
```

测试覆盖：

- 空查询 / 无命中；
- 按 `meta` / `value` 解析插件；
- 多段结果合并；
- 空结果过滤；
- 插件库缓存、重入拒绝与并发调用。
