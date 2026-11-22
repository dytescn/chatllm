---
name: chatllm-testenv
description: chatllm 本地测试环境：dytes test（0.2.3+）的 .dytes 目录布局、依赖在线下载规则、cargo test 环境准备、隔离与排查。
---

# chatllm 测试环境

本文件同步自 dytes 仓库的 `skills/testenv.md` 与 `docs/commands/test.md`（dytes 0.2.3 起），
并按 chatllm（`sys` 包）的实际情况调整。

## 1. 两个目录不要混淆

| 目录 | 来源 | 内容 | 是否入库 |
| --- | --- | --- | --- |
| `.dytes/env/` | `dytes test` 生成 | 依赖包与测试运行数据 | 否（`.gitignore` 已忽略 `.dytes/`） |

`cargo test` 直接使用 `.dytes/env/`，不再使用 `testenv/`：

- 当前调试包（chatllm）的产物始终使用 `target/release/` 下的本地文件，不会复制进 `.dytes/env/`；
- `dytes test` 不执行依赖包的 `setup.json` / init 脚本；
- `tests/common/mod.rs` 首次调用时把工作目录切到 `.dytes/env/`，幂等建表并写入基础对话种子；
- sys 包 `setup.json` 的格式与 dytes 处理规则见 skill `chatllm-setup`。

## 2. `dytes test` 的用法（0.2.3+）

```bash
dytes --version        # 需要 0.2.3 及以上；0.2.2 及以前是“复制本地依赖”的旧语义
dytes test             # 在包目录生成 .dytes/，自检 + 在线下载依赖 + 安装演练
```

常用参数：

| 参数 | 说明 |
| --- | --- |
| `--platform <platform>` | 选择 `platform-exports`，默认当前平台 |
| `--type <type>` | 只接受指定类型，不一致返回 `3` |
| `--manifest-only` | 允许没有 exports 的仅清单包 |
| `--work-dir <path>` | 演练目录，必须位于 `.dytes/` 内且不存在 |
| `--keep` | 保留演练目录便于排查 |
| `--json` | JSON 输出（有依赖下载进度时取 stdout 最后一行） |

行为要点：

1. 自动创建 `.dytes/`，并确保包根 `.gitignore` 含 `.dytes/`；
2. 复用 `dytes check` 校验清单、平台导出与设计插件；
3. 把 `dependencies-*` 的全部依赖（含传递依赖）与 `test-dependency` 合并，**在线下载最新版本**到 `.dytes/env/`；
4. 在 `.dytes/` 内演练当前包落盘并生成 `dytes.lock`；
5. 不修改真实安装目录，不执行 init 脚本。

退出码：`0` 全部通过；`1` 依赖下载/安装失败或 IO 失败；`2` 参数错误；
`3` 清单、导出、checksum、路径安全等校验失败。

## 3. `.dytes` 目录布局

```text
.dytes/
├── env/                        # 测试依赖环境（每次 test 清空重建）
│   ├── dytes.json              # 合并后的依赖清单
│   ├── dytes.lock              # 依赖安装记录
│   ├── libary/                 # sys / core 依赖产物
│   ├── web/<name>/
│   ├── plugins/<name>/
│   ├── apps/<name>/
│   └── models/<name>/
└── run-<pid>/
    └── dytes-test-<时间戳>/    # 当前包的安装演练目录（默认删除）
```

## 4. 依赖下载规则

1. **当前包不下载**：`chatllm.dll` / `libchatllm.so` / `libchatllm.dylib` 用本地构建产物。
2. **依赖全部下载**：`dytes.json` 的 `dependencies-sys` 已声明
   `database`、`designdb`、`kv_lib`、`plugin_lib`，`dytes test` 会把它们下载到
   `.dytes/env/libary/`。
3. **版本按 semver**：范围声明要求线上版本满足约束；精确声明低于线上版本时安装线上版本，
   锁文件记录声明值；精确声明高于线上版本直接报错。
4. **失败即中止**：任一依赖下载失败返回 `1`，不进入安装演练，也不回退到本地复制。

需要额外测试依赖时，在 `dytes.json` 中按类型声明（同名条目以 `test-dependency` 为准）：

```json
"test-dependency": {
  "sys": { "markparse": "0.1.0" },
  "design_plugins": { "cdr_check_txt": "1.1.0" }
}
```

## 5. 运行 `cargo test`

```bash
dytes test     # 依赖下载到 .dytes/env/libary/
cargo test
```

`tests/common/mod.rs` 的 `db_ready()` / `kv_ready()` 会：

- 默认读取 `.dytes/env/`，可用 `CHATLLM_TESTENV` 指向其它已准备的目录；
- 首次运行时切到该目录，自动创建 `dialogue` 表并写入 4 条种子对话；
- 缺少 `libary/database.dll`（或对应平台的库）时**默认让测试失败**并提示先运行
  `dytes test`，避免“静默 ok”；设置 `CHATLLM_TEST_ALLOW_SKIP=1` 可退回跳过模式。

`base_kv.rs` 会实际调用 `.dytes/env/libary/kv_lib` 的 `kv_collection`。

删除 `.dytes/` 后重新执行 `dytes test` 即可重建环境。

## 6. 隔离与凭证

- `dytes test` 的依赖只写入 `.dytes/env/`，不会碰真实安装目录（`%APPDATA%\dytes`、`~/.config/dytes`）。
- 需要独立配置时用 `DYTES_CONFIG` 指向临时 `config.toml`，并清除 `DYTES_S3_*`、
  `PIKSEL_PLATFORM` 等外部变量。
- 平台相关用例固定 `DYTES_PLATFORM=windows`，或显式传 `--platform`。
- 本地构建的 `dytes` 默认不含内置 S3 凭证；`dytes test` 需要读取 S3，凭证缺失会失败。
  配置方式：

  ```bash
  dytes config set s3.access_key
  dytes config set s3.secret_key
  # 或使用 DYTES_S3_ACCESS_KEY / DYTES_S3_SECRET_KEY
  ```

## 7. 排查

- **依赖下载失败**：先确认 `dytes --version` 是否 0.2.3+，再确认 S3 凭证与网络。
- **版本约束报错**：精确声明的版本高于线上版本时不会降级安装，需要先发布或放宽声明。
- **拿到旧包**：`packages.funxdata.com` 有 CDN 缓存，检查时给清单 URL 加时间戳：

  ```bash
  curl -fsSL "https://packages.funxdata.com/system/chatllm/dytes.json?t=$(date +%s)"
  ```

  `dytes` 本身走 S3 直连，不受 CDN 缓存影响。
- **`.dytes` 异常**：整个目录可随时删除，重新执行 `dytes test` 会重建。
