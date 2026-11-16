# plugins_lib FFI 调用方式

`plugins_lib` 是 `designplugin_lib` 的安装名。chatllm 通过
`dlopen ./libary/plugin_lib.dll` 直接调用其导出符号，不经过 cyctron 的 HTTP 层。
当前依赖版本以 `dytes.json` 为准（`plugin_lib: 1.1.0`）。

- 上游接口规范：`designplugin_lib/docs/ffi.md`（仓库同级目录）
- chatllm 封装：`src/utils/plugins_lib.rs`
- 上游源码导出：`designplugin_lib/src/ffi/*.rs`

## 1. 调用约定

```c
int symbol(const char *json_in, char *json_out);
```

| 项 | 约定 |
| --- | --- |
| 符号名 | 按操作命名（`get_*` / `post_*` / `patch_*` / `update_*` / `delete_*`），**不带** `post_` 统一前缀 |
| `json_in` | NUL 结尾的 UTF-8 JSON |
| `json_out` | 调用方缓冲区，至少 64 KiB |
| 返回值 | `0` 已写入；`-1` 输出指针为空；`-2` 响应达到 64 KiB 未写入 |
| 业务错误 | 响应 `code != 200`，FFI 仍返回 `0` |
| 响应 | `{"code":200,"data":{...},"msg":"success"}` |

`plugin_lib` 用相对路径加载 `./libary/database.dll` 与 `./libary/libdesigndb.dll`，
调用进程的 cwd 必须是包含 `libary/` 的安装根目录。`PLUGIN_LIB_DATABASE`
环境变量只覆盖 database 库路径。

## 2. 错误码

| code | 含义 |
| --- | --- |
| `400100` | `json_in` 为空指针 |
| `400101` | 请求不是 UTF-8 |
| `400102` | JSON 为空或解析失败、字段类型不匹配 |
| `400103` | 参数错误：缺必填字段、枚举值非法、无字段可更新 |
| `400404` | 插件 / 能力 / 技能不存在 |
| `400409` | 插件 UUID、能力 UUID、技能 ID 已存在 |
| `500201` | 依赖库加载或 SQL 执行失败 |
| `500300` | 插件目录、`dytes.json`、库文件读取失败 |
| `500299` | 内部 panic |

`src/utils/plugins_lib.rs` 导出 `CODE_BAD_REQUEST`（`400103`）、
`CODE_NOT_FOUND`（`400404`）、`CODE_CONFLICT`（`400409`）、
`CODE_DB_FAILED`（`500201`）、`CODE_FILE_FAILED`（`500300`）常量，
可用 `is_error_code(&error, code)` 判断。

## 3. 通用约定

- **字段命名**：请求与响应使用 snake_case；`desc`、`public` 是历史字段名。
- **分页**：`page` 默认 `1`，`page_size` 默认 `100`，范围 `1..=100`。
- **游标分页**：`get_plugins`、`get_functions`、`get_skills` 支持 `cursor`。
  把上一页返回的 `next_cursor` 原样传入即可继续；传 `cursor` 时忽略 `page`。
  `next_cursor` 没有下一页时为 `null`，列表同时返回 `has_more`。`cursor`
  是不透明字符串，不要解析或拼接。
- **布尔归一**：`enabled` / `is_enabled` / `is_system` / `use_ai` 等字段接受
  `true/false`、`1/0`、`"true"/"false"/"yes"/"no"/"on"/"off"`，统一为 `0/1`；
  其他已提供的值按 `0`。`post_plugins` 的 `enabled`、`post_plugins` /
  `update_plugins` 的 `public`、`mask` 是整数，不做归一，`mask` 是权限位。
- **JSON 展开**：`plugin_functions` 的 `params_schema` / `params_example` /
  `return_schema` / `return_example` 以 TEXT 存储，读取时自动解析为对象 / 数组。
- **物理删除**：`delete_*` 直接删除记录，不是软删除。`delete_plugins` 只删除
  `plugins` 行与 `plugin_functions`，不清理技能、提示词、任务流与版本记录。
- **时间戳**：`created_at` / `updated_at` 由服务端写入 Unix 秒，调用方不传。
- **列表返回**：`{list,total,page,page_size,next_cursor,has_more}`；详情、写入
  接口返回单行文档或操作结果。单次响应不得达到 64 KiB，列表应减小 `page_size`。

## 4. 接口清单

### 4.1 插件（`plugins` 表）

| 符号 | 请求 | 返回 |
| --- | --- | --- |
| `get_plugins` | 列表：`keyword?`、`status?`、`target?`、`platform?`、`page?`、`page_size?`、`cursor?`；详情：`uuid?` 或 `name?` | 列表分页结构；详情为插件行 |
| `post_plugins` | `name`（必填）、`uuid?`、`ouid?`、`plugins_type?`、`version?`、`desc?`、`create_by?`、`cover?`、`status?`、`public?`、`updated_by?`、`target?`、`platform?`、`mask?`、`enabled?` | `{uuid}` |
| `patch_plugins` | `uuid`（必填）、`enabled?`、`status?`、`cover?`、`mask?`（至少一个） | `{updated:true}` |
| `update_plugins` | `uuid`、`name`（均必填），其余主表字段可选 | `{updated:true}` |
| `delete_plugins` | `plugin_uuid?` 或 `name?` | `{plugin_uuid,name,deleted:true}` |

说明：

- `get_plugins` 只要请求中出现非空字符串 `uuid` 或 `name` 就返回详情，`uuid` 优先；
  详情不校验 `status` 等筛选条件。
- 列表按 `created_at DESC, uuid DESC` 排序；`keyword` 同时匹配 `name` 与 `desc`。
- `post_plugins` 省略 `uuid` 时生成 UUID v4；`status` 只允许 `draft`、`review`、
  `published`、`offline`、`deprecated`，省略时为 `draft`。
- `update_plugins` 是全量覆盖：未传的可选字段会写成 `NULL` 或默认值，调用前先读原值；
  不修改 `enabled`、`installed`、`install_path`、`use_ai`、`created_at`。

### 4.2 能力（`plugin_functions` 表）

| 符号 | 请求 | 返回 |
| --- | --- | --- |
| `get_functions` | `uuids?`（数组）、`plugin_uuid?`、`page?`、`page_size?`、`cursor?` | 分页结构 |
| `post_function` | `uuid`、`plugin_uuid`、`ffi_symbol`、`title`、`description`（均必填），四个 schema / example 可选，`use_ai?` | 新增后的能力行 |
| `update_function` | 同 `post_function`，必填字段必须全部提供 | 更新后的能力行 |
| `patch_function` | `uuid`（必填），其余字段可选且至少一个 | 更新后的能力行 |
| `delete_function` | `uuid`（必填） | `{uuid,deleted:true}` |

列表按 `uuid ASC` 排序；游标从该 `uuid` 之后继续。`params_schema` /
`return_schema` 缺省为 `{}`，`use_ai` 缺省为 `0`。

### 4.3 技能（designdb `skills` 集合）

| 符号 | 请求 | 返回 |
| --- | --- | --- |
| `get_skills` | `categories?`（数组）、`ids?`（数组）、`plugin_uuid?`、`enabled?`、`page?`、`page_size?`、`cursor?` | 分页结构，列表项为摘要，不含 `content` |
| `get_skill` | `id`（必填） | 完整技能文档（含 `content`） |
| `post_skills` | `id`、`plugin_uuid`、`name`、`category`（均必填），`description?`、`content?`、`trigger?`、`params_schema?`、`is_system?`、`is_enabled?`、`sort?`、`version?` | 新增后的技能文档 |
| `update_skills` | `id`（必填），其余字段可选且至少一个 | 更新后的技能文档，`version + 1` |
| `patch_skills` | 同 `update_skills` | 同 `update_skills` |
| `delete_skills` | `id`（必填） | `{id,deleted:true}` |

`get_skills` 不接受 `with_content`（传 `true` 返回 `400103`），需要正文时先取
`id`，再调用 `get_skill`。排序为 `is_system` 降序 → `category` 升序 → `sort`
升序 → `_id` 升序，游标基于该排序键。`content` 上限 1,000,000 字节，
`name` / `description` 分别上限 120 / 500 字符。

### 4.4 提示词（designdb `prompts` 集合）

| 符号 | 请求 | 返回 |
| --- | --- | --- |
| `get_prompts` | `plugin_uuid`（必填） | `{plugin_uuid,prompts}`；无文档时 `prompts: []` |
| `update_prompts` | `plugin_uuid`（必填）、`prompts`（数组，必填） | 读回后的 `{plugin_uuid,prompts}` |

每个 `plugin_uuid` 一条文档；`update_prompts` 整体覆盖，空数组也会覆盖为空。
每个 prompt 元素的 `content` 去首尾空白后按 UTF-8 编码不得超过 150 字节；
元素必须是 JSON 对象。该接口只写 designdb，不触发 kv 同步。

### 4.5 任务流（designdb `taskflow` 集合）

| 符号 | 请求 | 返回 |
| --- | --- | --- |
| `get_taskflow` | `{}` | `{taskflows}`；无文档时 `taskflows: []` |
| `update_taskflow` | `taskflows`（数组，必填） | 读回后的 `{taskflows}` |

`update_taskflow` 是全局整体覆盖，会替换所有插件贡献的任务流元素；单插件增量
应使用 `install_plugin` / `update_plugin`。每个元素必须包含 `plugin_uuid`、
`plugin_name`、`desc`、`ffi_symbol`。

### 4.6 安装与更新

| 符号 | 请求 | 返回 |
| --- | --- | --- |
| `install_plugin` | `dir`（必填） | 安装统计 |
| `update_plugin` | `dir`（必填）、`plugin_uuid?`、`name?`、`prune?` | 更新统计 |

这两个接口会批量写入 SQLite、designdb 与 kv，规则见上游
`designplugin_lib/docs/plugin-package.md`。

## 5. Rust 封装

`src/utils/plugins_lib.rs` 为 22 个导出符号各提供一个同名函数，参数统一为
`&Value` 并原样透传；另有 `call_ffi(symbol, request)` 用于直接调用。

| Rust 函数 | 导出符号 |
| --- | --- |
| `get_plugins` / `post_plugins` / `patch_plugins` / `update_plugins` / `delete_plugins` | 同名 |
| `get_functions` / `post_function` / `update_function` / `patch_function` / `delete_function` | 同名 |
| `get_skills` / `get_skill` / `post_skills` / `update_skills` / `patch_skills` / `delete_skills` | 同名 |
| `get_prompts` / `update_prompts` | 同名 |
| `get_taskflow` / `update_taskflow` | 同名 |
| `install_plugin` / `update_plugin` | 同名 |

分页 / 游标参数用 `list_request(page, page_size, cursor)` 构造，未提供的字段
不会出现在请求中；读取响应时用 `next_cursor(data)` 与 `has_more(data)`。

```rust
use crate::utils::plugins_lib::{self, list_request};
use serde_json::json;

let mut request = list_request(Some(1), Some(20), None);
request["status"] = json!("published");
let data = plugins_lib::get_plugins(&request)?;
println!("{} 个插件，还有下一页：{}", data["total"], plugins_lib::has_more(&data));

if let Some(cursor) = plugins_lib::next_cursor(&data) {
    let request = list_request(None, Some(20), Some(cursor));
    let _next = plugins_lib::get_plugins(&request)?;
}
```

详情、提示词与任务流直接构造对象：

```rust
let plugin = plugins_lib::get_plugins(&json!({ "uuid": "..." }))?;
let prompts = plugins_lib::get_prompts(&json!({ "plugin_uuid": "..." }))?;
let taskflow = plugins_lib::get_taskflow(&json!({}))?;

let skill = plugins_lib::get_skills(&json!({
    "categories": ["think"],
    "enabled": true,
    "cursor": null
}))?;
```

## 6. 与对话流程的关系

对话热路径（`think → chat_plugins → reply`）由 chatllm 直接读 kv 集合与
designdb 的 `taskflow` / `skills` 文档，不经过 `plugins_lib`。`plugins_lib`
面向插件管理、安装与提示词维护；其接口变化不影响 `post_chat` 的调用约定。

## 7. 版本

| 版本 | 变更 |
| --- | --- |
| `1.1.0` | `get_plugins` / `get_functions` / `get_skills` 增加游标分页；内部 kv 客户端迁移到 `kv_record` 记录接口；新增 MCP 规范 |
| `1.0.21` | database 改用 `db_*` C ABI 与 1 MiB 缓冲；designdb 缓冲提升到 16 MiB |

chatllm 从 `1.0.21` 升级到 `1.1.0`，22 个导出符号与 ABI 未变；新增的 `cursor`
字段可选，现有调用不受影响。
