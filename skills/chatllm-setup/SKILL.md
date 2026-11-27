---
name: chatllm-setup
description: chatllm 的 setup.json 规范：dytes 0.2.0+ 如何处理、直接 FFI 请求格式、path 规则、运行时兜底与验证方法。setup 禁止使用 HTTP 写法。
---

# chatllm `setup.json` 规范

适用于本仓库的 `setup.json`，以及任何新增初始化请求。

## 1. dytes 0.2.0+ 的处理方式

以下结论来自 dytes `v0.2.18` 源码（`src/update/script.rs`、`src/utils/ffi.rs`、
`src/update/update_setup.rs`）与 `docs/commands/{update,test,init}.md`：

1. `setup.json` 由 `dytes.json` 的 `init: ["./setup.json"]` 声明；
2. dytes 只解析顶层的 `requests` 数组，`version` 等其它字段被忽略；
3. 每个请求解析为：

   | 字段 | 作用 |
   | --- | --- |
   | `library` | 目标库（显式优先） |
   | `symbol` | 导出符号（显式优先） |
   | `url` | 兼容字段：取最后一段推断 library |
   | `headers.FFI-Symbol` | 兼容字段：推断 symbol（大小写不敏感） |
   | `method` | 兼容字段，当前忽略 |
   | `body` | 原样作为 JSON 请求传给库 |
   | `allow_error` | 业务码非 200 时只告警、继续执行 |

4. **执行方式是直接 FFI，不经过 HTTP 或 cyctron**（0.1.28 起，0.2.0–0.2.18 未改变）：
   - 加载 `libary/<library>.{dll,so,dylib}`；
   - 符号候选为 `<symbol>` 和 `post_<symbol>`；
   - 输出缓冲区 64 KiB，响应形如 `{code,data,msg}`；
   - `code == 200` 成功；否则 `allow_error` 为真时打印警告并继续，否则中止。
5. 日志格式为：

   ```text
   [setup 1/2] <url> -> database:db_update
   ```

   即使使用 `library`/`symbol` 字段，日志中的 `<url>` 位置也可能为空，这是正常的。
6. init 执行时机：
   - `dytes update` 在依赖更新后统一执行 sys 的 setup 与 web init；
   - 当前实现 `run_all_init_scripts` 会先 `start_cyctron()`，缺少 cyctron 时 update 的
     init 整体失败（0.2.12 只修复了单包 `dytes install` 的该场景）；
   - `dytes test` 明确**不执行** `setup.json`、不调用 FFI；
   - `dytes update --init` 的实现只扫描 `web/` 目录，不能用来执行 sys 包的 setup。
7. `path` 的语义是 database 库名，不是文件路径。database 1.0.6 只接受
   `[A-Za-z0-9_-]`，带 `./` 会返回 `500100 invalid database name`。

## 2. 本项目硬性规范

- **禁止 HTTP 写法**：不得使用 `url`、`method`、`headers.FFI-Symbol`；
  必须使用 `library` + `symbol` + `body` 的直接 FFI 格式。
- `path` 使用库名，例如 `"dialogue"`，不得写成 `"./dialogue"`。
- SQL 必须可重复执行：
  - 建表使用 `CREATE TABLE IF NOT EXISTS`；
  - 补列前先用 `SELECT ... FROM pragma_table_info('<表>')` 判断列是否存在，再执行
    `ALTER TABLE ... ADD COLUMN`；
  - 不得用 `"allow_error": true` 掩盖重复列错误；建表已含的列不要再无条件补列。
- `body` 必须与运行时 FFI 请求一致，字段名、`path`、SQL 不得分叉。
- `setup.json` 仍是安装期初始化的主入口；运行时兜底不能替代它。
- 新增表或列时，必须同步更新：
  1. `setup.json`；
  2. 运行时 `ensure_schema()`（见 `src/utils/dialogue.rs`）；
  3. `docs/db/*.md`；
  4. `.dytes` 环境验证。

## 3. 正确示例

```json
{
  "requests": [
    {
      "library": "database",
      "symbol": "db_update",
      "body": {
        "path": "dialogue",
        "sql": "CREATE TABLE IF NOT EXISTS dialogue (...);"
      }
    }
  ]
}
```

建表语句已包含 `system_prompt`，旧库补列由运行时 `ensure_schema()` 完成，不再在
`setup.json` 中重复执行 `ALTER TABLE`，因此不需要 `allow_error`。

## 4. 运行时兜底

`src/utils/dialogue.rs` 的 `ensure_schema()` 在首次访问对话接口时幂等执行
`CREATE TABLE IF NOT EXISTS`，并在旧库缺少 `system_prompt` 列时先查
`pragma_table_info` 再补列，避免宿主没有执行 init（例如缺少 cyctron）时返回
`no such table: dialogue`。它使用 `OnceLock` 缓存成功状态，失败时下次重试；
查询或补列的真实错误会向上返回，不再被吞掉。

## 5. 验证方法

```bash
# 1. 结构与依赖落盘检查（不执行 setup）
dytes test

# 2. 按 dytes 规则实际执行 setup
#    工作目录设为 .dytes/env，使 path "dialogue" 落到 .dytes/env/database/
#    加载 .dytes/env/libary/database.dll，依次调用 library:symbol
#    最后用 PRAGMA table_info(dialogue) 校验列

# 3. 回归
cargo test
```

## 6. 常见错误

| 现象 | 原因 |
| --- | --- |
| `no such table: dialogue` | init 未执行（常见于缺少 cyctron），或 setup 从未落盘 |
| `500100 invalid database name` | `path` 带了 `./` 或路径分隔符 |
| `未找到 cyctron` | dytes update 的 init 流程问题；chatllm 运行时已不依赖它 |
| 请求被跳过且无报错 | 检查是否把字段写进了 `headers`，而不是顶层的 `library`/`symbol` |
| `duplicate column name: xxx` | 补列语句未先查 `pragma_table_info`，重复执行 init 时触发 |
