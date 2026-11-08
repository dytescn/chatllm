# chatllm

`chatllm` 是一个以 Rust 编写、编译为动态库的对话存储与编排模块。它导出 C ABI
`post_*` 函数，由宿主（Piksel / cyctron）通过 `POST /chatllm` + `FFI-Symbol`
调用。运行时 cwd 为安装根目录，`./libary`、`./database`、`./designdb`、
`./plugins` 均相对该目录解析。

版本以 `Cargo.toml` 为准；发布清单见 `dytes.json`。

## 功能

- 新建 / 续聊对话，保存 user 与 assistant 消息；
- 对话列表、搜索、更新、详情与游标分页；
- 对话前置检查：确认 AI 供应商与 AppKey 已配置；
- `check → think → plugins → reply` 对话编排；
- `call_chat`：按 kv 分类直接定位并调用插件，跳过 think_base / reply；
- 消息 Markdown 渲染为 `content_html`。

供应商调用统一由 `cloud.rs` 处理：`reply` 命中供应商插件时调用独立的
`cloud_lib` 系统库（内部再分发 DeepSeek / SiliconFlow），其他插件按插件协议执行。

## 对话流程

```text
chat
  │
  ├─ check：读取 info.db.setting
  │    缺少 ai_supplier 或对应 AppKey
  │      → 不创建对话，直接返回 content + route="/setting"
  │
  ├─ think：检索 chat_think，按 "|" 拆分
  │
  ├─ plugins：逐段检索 chat_plugin 并执行插件
  │    未命中 / 无有效段 → 跳过 plugins
  │
  └─ reply：检索 chat_reply，命中插件调用 cloud；未命中回退 reply_base
```

供应商检查成功后会缓存 `info.db` 的修改时间与大小；`info.db` 变化时缓存自动失效。

`call_chat` 是独立的轻量流程：不执行 check / think / reply，直接用 `category`
作为 kv 集合名、`prompts` 作为检索词取 Top1 插件，并把
`{plugins_uid, category, prompts, context}` 原样交给插件，返回插件 `data`。
细节见 [`docs/ffi.md`](docs/ffi.md#26-call_chat--post_call_chat)。

## FFI

所有接口统一返回 `{code, data, msg}`，成功 `code = 200`。结果写入调用方提供的
64 KiB 缓冲区。

| FFI-Symbol | 导出函数 | 说明 |
| --- | --- | --- |
| `chat` | `post_chat` | 新建 / 续聊，执行对话流程并写回 assistant |
| `dialogue_list` | `post_dialogue_list` | 对话分页列表 |
| `dialogue_update` | `post_dialogue_update` | 更新标题、显示状态、置顶 / 归档 |
| `dialogue_search` | `post_dialogue_search` | 按标题 / keyword 搜索 |
| `chat_detail` | `post_chat_detail` | 读取对话与消息，支持游标分页 |
| `call_chat` | `post_call_chat` | 按 kv 分类直接调用插件 |

接口细节见 [`docs/ffi.md`](docs/ffi.md)。

## 存储

| 数据 | 位置 |
| --- | --- |
| 对话索引 | `database/dialogue.db` → `dialogue` |
| 对话消息 | `designdb/<dialogue_id>` → `messages` |
| 设置 | `database/info.db` → `setting` |

## 外部依赖

| 依赖 | 用途 |
| --- | --- |
| `database` | SQLite 访问 |
| `libdesigndb` | 按对话 UUID 存储消息 |
| `kv_lib` | `chat_think` / `chat_plugin` 检索 |
| `parse` | Markdown → HTML |
| `cloud_lib` | DeepSeek / SiliconFlow 云端调用（独立 `cloud_lib` 包） |
| `info_lib` | 设置页 / 配置读写 |
| `plugin_lib` | 插件能力清单 |

依赖声明见 `dytes.json` 的 `dependencies-sys`。初始化文件为 `setup.json`（创建
`dialogue` 表）；本地 `testenv/` 仍保留 `setup.ts` 作为测试重置脚本。

## 构建与测试

```bash
cargo build --release
cargo test
```

`cargo test` 包含单元测试与 `testenv` 数据测试；缺少 `testenv/libary/*` 或
`testenv/database/*` 时，相关用例会自动跳过并打印原因。`call_chat` 冒烟测试
（`tests/base_call_chat.rs`）额外需要 `kv_lib.dll` 与
`plugins/do_test_chat/do_test_chat.dll`。

产物：`target/release/chatllm.dll`（Linux `libchatllm.so`、macOS
`libchatllm.dylib`）。

## 文档

- [`docs/ffi.md`](docs/ffi.md)：FFI 接口、插件直接调用与错误码
- [`docs/db.md`](docs/db.md)：对话存储结构
- [`docs/kv_lib.md`](docs/kv_lib.md)：kv_lib 调用方式
- [`docs/testenv.md`](docs/testenv.md)：本地测试环境
- [`docs/test_base_message.md`](docs/test_base_message.md)：`chat` 接口测试
- [`docs/test_base_dialogue.md`](docs/test_base_dialogue.md)：对话列表 / 更新测试
- [`docs/test_base_call_chat.md`](docs/test_base_call_chat.md)：`call_chat` 冒烟测试
- [`runflow/base.md`](runflow/base.md)：基础对话流程
- [`runflow/think.md`](runflow/think.md)：think_base / reply_base 资源包流程
- [`runflow/plugins.md`](runflow/plugins.md)：插件检索与执行
- [`runflow/reply.md`](runflow/reply.md)：chat_reply 与最终回复
- [`todo/TODO.md`](todo/TODO.md)：路线图与待办
- [`skills/`](skills/)：本地环境搭建与测试操作手册

## 已知限制

- 外部库路径固定为 `./libary/*`，暂未配置化；
- 输出缓冲区固定 64 KiB，超长结果会被拒绝（`-2`）；
- 插件与系统库加载后常驻缓存，进程内不主动卸载；
- 同一线程内禁止插件重入调用（返回 `400304`），跨线程回调需插件自身可重入；
- `dialogue` 使用文本时间，`messages` 使用 Unix 毫秒；
- 错误码仍分散在各模块。
