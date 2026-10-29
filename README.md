# chatllm

本地 AI 对话与向量检索能力库，Rust `cdylib`，由 cyctron 以 `POST /chatllm` + `FFI-Symbol` 加载。

## 能力

- 对话：`post_chat`，调用 siliconflow 生成回复并落库；
- 对话列表 / 搜索 / 更新 / 详情：`dialogue_*`、`chat_detail`；
- 向量：插件能力 `plugin_embed` / `plugin_search`、素材 `material_embed` / `material_search` / `material_delete`、提示词 `embed_prompts`；
- 本地检索：`turbo_search_query`（turbovec）；
- 模型：`qwen3-embed`（1024 维，`models/qwen3-embed/`）。

## 存储

| 数据 | 位置 |
| --- | --- |
| 对话索引 | `database/dialogue.db` → `dialogue` |
| 消息 | `designdb/<dialogue_id>` → `messages` |
| 向量分片 | `database/embedding_<category>_<n>.db` → `prompts_embedding` |
| 技能读取 | `plugin_lib.get_skills`（`plugins.db.skills`） |

遗留项：`dialogue.db` 仍创建 `session` / `skills` 表，`sessions_*` / `skills_*` 仍导出，待清理。

## 构建

```bash
# 需要 libclang（LLVM）与 CMake / MSVC
LIBCLANG_PATH="C:\Program Files\LLVM\bin" cargo build --release
```

产物：`target/release/chatllm.dll`，由 `dytes publish` 发布为 `system/chatllm`。

## 文档

文档在 `dytes_sys_operations` 仓库：

- `sysinfo/sys/chatllm/instro.md` —— 模块、依赖、构建与限制
- `sysinfo/sys/chatllm/ffi.md` —— FFI 接口与错误码
- `sysinfo/sys/chatllm/database.md` —— 当前数据结构
- `sysinfo/sys/chatllm/db_todo.md` —— 迁移与清理待办

## 已知限制

- `sessions_*` / `skills_*` 仍导出；`setup.ts` 建的是 `session` 表，`utils/sessions.rs` 查询 `sessions` 表，表名不一致；
- 旧 SQLite 对话尚未迁移（G-40）；
- `Cargo.lock` 未入库，构建不完全可复现。
