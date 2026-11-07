# 基础对话流程

本文描述一次 `chat` 调用的完整路径：`check → think → plugins → reply`。

## 流程

```text
POST /chatllm  FFI-Symbol: chat
        │
        ▼
check：读取 info.db.setting.ai_supplier + AppKey
        │
        ├─ 缺失 / 未知 / 无 token
        │     → 直接返回 content 与 route="/setting"
        │     → 不创建对话、不写消息
        │
        ▼
think：检索 chat_think，取 Top1 value，按 "|" 拆分
        │
        ├─ 未命中 / 无有效段
        │     → 跳过 plugins，prompt 交给 reply
        │
        ▼
plugins：逐段检索 chat_plugin 并调用 Top1 插件
        │
        ├─ 单段失败 → 该段结果为错误文本
        │
        ▼
reply：合并插件结果，检索 plugins_reply；命中插件调用 cloud，未命中回退 reply_base
        │
        ▼
写入 assistant，返回 uuid / created
```

关键行为：

- user 消息在 check 通过后写入；
- 配置缺失时 `chat` 仍返回 `code = 200`，但 `uuid = null`、`created = false`；
- think 按整段数组处理，不是只取最后一段；
- plugins 每段独立执行，结果以空行合并；
- 供应商类插件（`deepseek` / `siliconflow_t2t`，可带 `lib` 前缀）统一走
  `cloud::chat_with`，由独立系统库 `cloud_lib` 调用；
- 插件或供应商错误只影响 assistant 内容，不阻断消息存储。

## 配置

`info.db.setting`：

| key | 说明 |
| --- | --- |
| `ai_supplier` | `deepseek` / `siliconflow_t2t` |
| `deepseek_app_key` | DeepSeek token |
| `silicon_app_key` | SiliconFlow token |

`chat_think`：think 检索源，命中记录的 `value` 可包含 `|` 分段。
`chat_plugin`：插件检索源，记录需包含 `lib_name` / `ffi_symbol`。

## 本地验证

```bash
cargo build --release
cp target/release/chatllm.dll testenv/libary/chatllm.dll

cd testenv
./cyctron.exe start
```

```bash
# 未配置供应商：直接返回设置提示
deno run --allow-net ts/test_check.ts

# 配置后完整流程
deno run --allow-net ts/test_pipeline.ts

# 供应商 / AppKey 组合
deno run --allow-net ts/test_cloud.ts

# 真实供应商（token 目录由 FUNXTOKEN_DIR 指定）
deno run --allow-net --allow-read --allow-env ts/test_live.ts
```

手工调用：

```bash
curl -s -X POST http://127.0.0.1:44944/chatllm \
  -H "Content-Type: application/libary" \
  -H "FFI-Symbol: chat" \
  -d '{"prompt":"hello","title":"base"}'
```

## 常见问题

| 现象 | 处理 |
| --- | --- |
| 返回“请选择设置AI供应商” | 检查 `ai_supplier` 与对应 AppKey |
| assistant 未使用插件结果 | `chat_think` 未命中，或分段后没有有效段 |
| assistant 为“我收到你的信息了” | `reply_base` 资源包不可用 |
| `/chatllm` 返回 404 | 检查 `testenv/libary/chatllm.dll`，覆盖后重启 cyctron |
| `400101 invalid utf-8` | 终端编码问题，改用 Deno / Node 发送请求 |
