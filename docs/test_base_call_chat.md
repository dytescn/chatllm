# test_base_call_chat

> 对应测试文件：`tests/base_call_chat.rs`。
> 环境准备见 `docs/testenv.md`；请求结构与事件协议见 `docs/ffi.md` 2.6。

## 1. 覆盖接口

- `call_chat`（导出函数 `post_call_chat`）：入参
  `{plugins_uid?, category, prompts?, context?}`；
- 底层 `utils::plugins::call_plugin_data`：插件调用与 `data` 原样返回。

## 2. 用例

| 用例 | 输入 | 预期 |
| --- | --- | --- |
| `invalid_json_returns_400102` | `{not-json` | `code = 400102` |
| `missing_category_returns_400102` | `{"prompts":"测试"}` | `code = 400102` |
| `empty_category_returns_400304` | `{"category":"   "}` | `code = 400304`，`msg` 含 `category` |
| `unknown_collection_returns_400304` | `{"category":"no_such_collection_chatllm_test","prompts":"测试"}` | `code = 400304` |
| `plugin_call_is_forwarded_and_errors_mapped` | `{"category":"chat_plugin","prompts":"do_test_chat","context":{"a":1}}` | 命中 `do_test_chat`；插件拒绝四字段请求，映射为 `code = 400304` |
| `plugin_data_is_returned_verbatim` | `plugins::call_plugin_data("do_test_chat","chat",{"prompt":"你好"})` | `data.tag = do_chat_test`，`data.input = 你好`，`data.content` / `data.time` 存在 |
| `plugin_library_is_cached_across_calls` | 连续 3 次调用 `do_test_chat` | 均成功，`tag = do_chat_test` |
| `missing_plugin_returns_error` | `no_such_plugin_chatllm` | 返回 `Err`，错误信息含插件名 |
| `concurrent_plugin_calls_are_safe` | 4 线程并发调用 `do_test_chat` | 全部成功，缓存锁无死锁 |

## 3. 依赖

- `testenv/libary/kv_lib.dll`：分类检索；
- `testenv/plugins/do_test_chat/do_test_chat.dll`：测试插件；
- `chat_plugin` 集合中至少一条 `do_test_chat` 记录。

缺少上述文件时，用例通过 `common::kv_ready()` 自动跳过并打印原因。

## 4. 运行

```bash
# 仓库根目录；kv_ready() 会把 cwd 切到 testenv
cargo test --test base_call_chat

# 查看每个请求与插件日志
cargo test --test base_call_chat -- --nocapture --test-threads=1
```

## 5. 说明

`do_test_chat` 只接受 `prompt` / `messages`，不接受 `call_chat` 的四字段请求，
因此成功路径由 `plugins::call_plugin_data` 直接覆盖；`post_call_chat` 一侧
覆盖参数校验、kv 检索、插件调用与错误映射。
