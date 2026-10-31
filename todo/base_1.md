# base_1 基线测试流程

> 适用版本：chatllm 1.0.8+（`chat` 写 user 消息后检索 `chat_plugin`，有插件时调用并写回回复）
> 最后执行：2026-10-31（Windows / Deno 2.9.6）

## 目的

验证 chatllm 的最小对话闭环在 `testenv` 中可用：

| 序号 | 接口 | 断言要点 |
| --- | --- | --- |
| 1 | `chat`（新建） | `code=200`、`created=true`、`uuid` 为 36 位、`content` / `content_html` 为空 |
| 2 | `chat`（续聊） | `created=false`、`uuid` 一致 |
| 3 | `dialogue_list` | 能查到该对话，`title` / `keyword` 正确，`message_count=4`、`last_seq=4` |
| 4 | `chat_detail` | 4 条消息：user / assistant / user / assistant；assistant 内容非空（安装提示或插件回复） |
| 5 | `dialogue_search` | 按唯一 keyword 命中，`total=1` |
| 6 | `dialogue_update` | 改标题 + `is_archived=1` 返回 200 |
| 7 | 归档后 | `dialogue_list` / `dialogue_search` 均不再返回该对话 |

脚本：`ts/base_1.ts`（复用 `ts/client.ts` 的 `call` / `section` / `assert`）。

## 前置条件

1. 仓库根目录下的 `testenv` 已就绪，至少包含：

   ```
   testenv/
   ├── cyctron.exe
   ├── database/dialogue.db
   ├── database/info.db
   ├── designdb/
   └── libary/{chatllm,database,kv_lib,libdesigndb,parse}.dll
   ```

   同时需要 `models/qwen3-embed/qwen3-embedding-0.6.gguf`（`chat` 内部检索
   `chat_plugin` 时使用），或设置 `KV_EMBED_MODEL`。

2. 已安装 Deno（`deno --version`），脚本通过 `fetch` 访问本机 HTTP。
3. 端口 `44944` 未被占用。`ts/client.ts` 中 `BASE` 固定为
   `http://127.0.0.1:44944/chatllm`，不要改用其他端口，除非同步修改 `client.ts`。
4. `testenv/libary/chatllm.dll` 必须是**当前源码**的构建产物。旧版 dll 仍会尝试调用
   siliconflow，导致 `chat` 返回 `400400`。

## 执行流程

### 0. 部署被测库（首次或代码变更后）

```bash
cargo build --release
# 若 testenv 正在运行，先停止，否则 Windows 下 dll 被占用无法覆盖
cp target/release/chatllm.dll testenv/libary/chatllm.dll
```

### 1. 启动 testenv

```bash
cd testenv
./cyctron.exe start          # 默认监听 0.0.0.0:44944
```

看到以下日志即启动成功：

```text
HTTP + WS server started on http://0.0.0.0:44944
```

### 2. 运行 base_1 测试

另开一个终端，在**仓库根目录**执行：

```bash
deno run --allow-net ts/base_1.ts
```

### 3. 停止 testenv

```bash
cd testenv
./cyctron.exe stop
```

### 一键执行（bash）

```bash
cd testenv && ./cyctron.exe start && cd .. \
  && deno run --allow-net ts/base_1.ts; rc=$?; \
  cd testenv && ./cyctron.exe stop; exit $rc
```

## 结果判读

- 每个断言输出 `✅ <描述>`；失败输出 `❌ <描述>`，且进程退出码为 `1`。
- 全部通过时退出码为 `0`，末尾无 `❌`。
- 任一断言失败时，先看对应步骤打印的原始响应（`console.log` 输出）。

预期关键响应片段：

```json
// chat 新建
{ "code": 200, "data": { "created": true, "content": "", "content_html": "", "uuid": "..." } }

// chat_detail
{ "code": 200, "data": { "first_id": 1, "last_id": 4, "has_more_before": false, "list": [ ... ] } }
```

## 数据副作用

- 每次执行会新建一个标题/关键字为 `base_1-<timestamp>` 的对话，并向
  `designdb/<dialogue_id>` 写入 4 条消息：每次 `chat` 各写 1 条 user +
  1 条 assistant。assistant 可能是安装提示、候选列表或插件/供应商回复，
  取决于 `chat_plugin` 与 `ai_supplier` 的配置。
- 测试结束会把该对话置为归档（`is_archived = 1`），不再出现在列表与搜索结果中；
  消息库不会自动删除。
- 若需要完全清理，可手工删除 `database/dialogue.db` 中对应 `dialogue` 行和
  `designdb/<dialogue_id>` 目录。

## 常见问题

| 现象 | 原因 / 处理 |
| --- | --- |
| `Connection refused` / `fetch failed` | testenv 未启动，或端口不是 44944 |
| `chat` 返回 `400400 siliconflow chat failed` | testenv 中仍是旧版 `chatllm.dll`；按步骤 0 重新构建并覆盖 |
| `code=400306` | `designdb` 读写失败，检查 `libary/libdesigndb.dll` 与 `designdb/` 目录权限 |
| `content_html` 为空 | `libary/parse.dll` 缺失；`chat_detail` 会降级为空字符串，不阻断请求 |
| 端口被占用 | 先执行 `testenv/cyctron.exe stop`，或排查占用 44944 的进程 |
| 断言 `message_count = 4` 失败 | 该 `uuid` 被重复使用、`chat_plugin` 检索失败，或 `database/dialogue.db` 计数未同步 |
| assistant 为 `调用聊天插件失败` | 检查 `plugins/do_test_chat/` 与 `libary/libsiliconflow_t2t.dll` 是否安装；或改用安装提示场景 |

## 执行记录

- 2026-10-31：`cargo build --release` 后覆盖 `testenv/libary/chatllm.dll`，
  `deno run --allow-net ts/base_1.ts` 全部断言通过，退出码 `0`。
  测试对话 `e37d40ec-0daa-4b7e-bb92-52ebb9e312ea` 已归档
  （4 条消息：2 user + 2 assistant）。
- 2026-10-31：早期版本测试对话 `071d4a5f-fa00-4322-83af-7743a944582c` 已归档。
