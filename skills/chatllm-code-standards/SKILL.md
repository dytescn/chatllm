---
name: chatllm-code-standards
description: chatllm 代码组织规范：src 只放实现、不写测试；所有测试放在 tests/；src 下的 mod.rs 只允许模块引用。
---

# chatllm 代码组织规范

适用于本仓库的 Rust 代码。新增或修改代码时按以下三条执行。

## 1. `src/` 不写测试代码

`src/` 下只放可交付的实现代码，不允许出现：

- `#[cfg(test)]`；
- `mod tests { ... }`；
- `#[test]` / `#[tokio::test]` 等测试函数。

示例（不允许）：

```rust
// src/utils/foo.rs
pub fn parse(input: &str) -> Option<&str> { ... }

#[cfg(test)]          // ❌ src 中不允许
mod tests { ... }
```

## 2. 测试代码统一放 `tests/`

所有测试放到仓库根目录的 `tests/`：

- 每个测试文件用 `tests/<模块名>.rs` 命名，例如 `tests/utils_taskflow.rs`；
- 通过 `use chatllm::...` 调用库的公开 API；
- 需要多个测试共享的辅助代码放 `tests/common/mod.rs`；
- 测试目标如果原本是私有纯函数，应改为 `pub` 并补充文档注释，或在 `tests/` 中通过公开 API 覆盖，不要为了测试把测试代码留在 `src/`；
- **新增或修改实现时，必须在 `tests/` 同步增加/更新用例**，不允许只改 `src/` 而不带测试；
- `src/` 中原有的 `#[cfg(test)] mod tests` 应迁移到 `tests/<模块名>.rs`，迁移后立即删除，不保留双份测试。

示例：

```rust
// tests/utils_foo.rs
use chatllm::utils::foo::parse;

#[test]
fn parses_input() {
    assert_eq!(parse("x"), Some("x"));
}
```

## 3. `src/**/mod.rs` 只放模块引用

`src/` 下的 `mod.rs` 只允许声明子模块，不写函数、常量、类型、测试或其他逻辑：

```rust
// src/utils/mod.rs
pub mod chat_plugins;
pub mod plugins_lib;
pub mod taskflow;
```

不允许：

```rust
// src/utils/mod.rs
pub mod plugins;

pub fn helper() {}          // ❌ mod 中不写代码
pub const VALUE: i32 = 1;   // ❌
```

`tests/common/mod.rs` 属于测试辅助模块，不受第 3 条限制；测试辅助代码仍应只放在 `tests/`。

## 自检

提交前执行：

```bash
# src 中不应有测试代码
grep -rn "#\[cfg(test)\]\|#\[test\]" src

# src 下的 mod.rs 只应有 pub mod 行
find src -name mod.rs -exec grep -Hn . {} +

# 测试必须全部通过，且新增实现的用例在 tests/ 中
cargo test
```

第一条应无输出；第二条只应出现 `pub mod` 声明。
