# Roadmap

## 近期

- [ ] **think / reply 资源包安装**
  - 将 `think_base` / `reply_base` 的 `prompts/*.json` 写入 `chat_think` / `chat_reply`；
  - 提供可重复执行的安装与校验脚本。

- [ ] **system_prompt 透传**
  - 当前只写入 `dialogue.system_prompt`；
  - 供应商调用时把对话级 system 传给模型。

- [ ] **reply 清洗泛化**
  - 当前清洗规则针对测试插件输出；
  - 为真实插件输出补充通用清洗策略。

## 质量

- [ ] **统一错误码**
  - 位置：`src/utils/errcode.rs`、各 `src/ffi/*.rs`；
  - 现状：`ChatError` 覆盖对话与插件错误（含 `400304`），FFI 解析错误仍直接写字面量；
  - 验收：错误码集中定义，FFI 只做映射，补充错误码表测试。

- [ ] **SQL 参数化**
  - 位置：`src/utils/db.rs`、`dialogue.rs`、`message.rs`；
  - 现状：部分 SQL 通过 `format!` 拼接；
  - 验收：用户输入不直接进入 SQL 字符串，补充转义/参数化测试。

- [ ] **外部库路径可配置**
  - 位置：`db.rs`、`designdb.rs`、`kv.rs`、`cloud.rs`、`markparse.rs`；
  - 现状：固定为 `./libary/*`；
  - 验收：支持环境变量或配置覆盖，缺失时返回明确错误。

- [ ] **日志分级**
  - 现状：各模块使用 `println!` 输出调试信息，包含完整请求/响应片段；
  - 验收：使用统一日志宏与级别控制，默认不输出敏感内容，保留关键阶段日志。

- [ ] **输出缓冲区**
  - 位置：`cjson::write_json_out` 及各库调用；
  - 现状：固定 64 KiB，超限返回 `-2`；
  - 验收：超限错误可定位到具体接口，必要时支持分块读取。

- [ ] **UUID 校验**
  - 位置：`src/utils/dialogue.rs::valid_uuid`；
  - 现状：只校验长度与字符集；
  - 验收：校验标准 UUID 分组与版本位，并补充非法样例测试。

- [ ] **构建可复现**
  - 现状：`Cargo.lock` 被忽略；
  - 验收：明确是否入库；若入库，CI 使用锁定依赖构建。

- [ ] **CI 检查**
  - 现状：发布工作流以构建为主；
  - 验收：增加 `cargo fmt --check`、`cargo test`、`cargo clippy`，并保留发布构建。

## 已完成

- [x] 对话消息迁移到 `designdb/<dialogue_id>`；
- [x] `check → think → plugins → reply` 流程；
- [x] 供应商配置缺失时直接返回 `/setting` 提示；
- [x] 配置检查成功缓存，`info.db` 变化自动失效；
- [x] 供应商（deepseek / siliconflow_t2t）统一走 `cloud`；
- [x] reply 优先检索 `plugins_reply`，支持 `plugins_uid` 对话复用 / 创建；
- [x] `reply_base` 资源包清洗与系统对话；
- [x] 64 KiB 输出缓冲区与 FFI 错误码；
- [x] `call_chat`：按 kv 分类直接调用插件，跳过 think_base / reply；
- [x] 插件库缓存复用（进程内常驻）与同线程重入保护；
- [x] `call_chat` 单元测试与 `testenv` 冒烟测试。
