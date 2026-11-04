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
```

测试覆盖：

- 空查询 / 无命中；
- 按 `meta` / `value` 解析插件；
- 多段结果合并；
- 空结果过滤。
