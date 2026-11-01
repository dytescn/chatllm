# test_base_message

> 对应测试文件：`tests/base_message.rs`。
> 环境准备与建库流程见 `docs/testenv.md`。

## 1. 覆盖接口

- `post_message`（FFI-Symbol：`post_message`）：入参 `ChatFlow`（`prompt` 必填，`uuid` 非空时续聊已有对话）。

## 2. 用例

| 用例 | 输入 | 预期 |
| --- | --- | --- |
| `post_message_creates_then_continues` | `{"prompt":"你好，介绍一下自己","title":"测试新建"}`，随后 `{"prompt":"继续","uuid":<上一步 uuid>}` | 首次 `created = true` 且 `uuid` 为 36 位；续聊 `created = false` 且 `uuid` 不变；列表中该对话 `message_count = 2`、`last_seq = 2`；测试末尾调用 `utils::dialogue::delete_dialogue` 清理 |
| `post_message_rejects_unknown_uuid` | `{"prompt":"x","uuid":"9999…"}` | `code = 400303` |
| `missing_prompt_returns_400102` | `{"uuid":"x"}` | `code = 400102` |

## 3. 运行

```bash
# 仓库根目录；数据测试会自动把 cwd 切到 testenv
cargo test --test base_message

# 查看每个请求的返回
cargo test --test base_message -- --nocapture --test-threads=1
```

每次调用会打印三行，便于核对：

```text
request : {"prompt":"你好，介绍一下自己","title":"测试新建"}
rc      : 0
response: {"code":200,"data":{"created":true,"max_tokens":null,"prompt":"你好，介绍一下自己","uuid":"…"},"msg":"success"}
```

## 4. 依赖与清理

- 需要 `testenv/database/dialogue.db`，以及 `testenv/libary/database.dll`、`libdesigndb.dll`。
- 消息写入 `./designdb/<dialogue_id>` 的 `messages` 集合；用例结束会删除新建对话及其消息。
