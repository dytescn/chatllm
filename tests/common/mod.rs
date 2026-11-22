// tests/common/mod.rs —— FFI 接口的调用辅助
#![allow(dead_code)]

use serde_json::Value;
use std::ffi::{c_char, c_int, CStr, CString};
use std::path::{Path, PathBuf};
use std::sync::Once;

/// 测试环境目录：默认仓库根的 `.dytes/env`（由 `dytes test` 生成），
/// 可用 `CHATLLM_TESTENV` 覆盖。
pub fn testenv_dir() -> PathBuf {
    match std::env::var("CHATLLM_TESTENV") {
        Ok(dir) if !dir.trim().is_empty() => PathBuf::from(dir),
        _ => Path::new(env!("CARGO_MANIFEST_DIR")).join(".dytes").join("env"),
    }
}

#[cfg(windows)]
const DATABASE_LIB: &str = "database.dll";
#[cfg(target_os = "linux")]
const DATABASE_LIB: &str = "database.so";
#[cfg(target_os = "macos")]
const DATABASE_LIB: &str = "database.dylib";

#[cfg(windows)]
const KV_LIB: &str = "kv_lib.dll";
#[cfg(target_os = "linux")]
const KV_LIB: &str = "kv_lib.so";
#[cfg(target_os = "macos")]
const KV_LIB: &str = "kv_lib.dylib";

/// 缺少依赖时默认让测试失败，避免“静默 ok”。
/// 需要宽容模式时设置 `CHATLLM_TEST_ALLOW_SKIP=1`。
fn require_env(path: &Path, what: &str) -> bool {
    if path.is_file() {
        return true;
    }
    let message = format!(
        "测试环境未就绪：缺少 {}。请先运行 `dytes test`，或用 CHATLLM_TESTENV 指向已准备的 .dytes/env。",
        path.display()
    );
    if std::env::var("CHATLLM_TEST_ALLOW_SKIP").as_deref() == Ok("1") {
        eprintln!("跳过 {what}：{message}");
        false
    } else {
        panic!("{message}");
    }
}

/// 数据调用测试使用 `.dytes/env` 下的 database 库。
///
/// `database` 以相对路径加载 `./database/*.db`，因此首次调用时把进程工作目录
/// 切到 `.dytes/env`，并幂等建表、写入基础对话种子。
pub fn db_ready() -> bool {
    let env = testenv_dir();
    if !require_env(&env.join("libary").join(DATABASE_LIB), "database 测试") {
        return false;
    }
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        std::env::set_current_dir(&env).expect("切换测试环境目录失败");
        prepare_dialogue_db();
    });
    true
}

/// kv 调用测试使用 `.dytes/env` 下的 kv_lib。
pub fn kv_ready() -> bool {
    let env = testenv_dir();
    if !require_env(&env.join("libary").join(KV_LIB), "kv 测试") {
        return false;
    }
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        std::env::set_current_dir(&env).expect("切换测试环境目录失败");
    });
    true
}

const DIALOGUE_SCHEMA: &str = "CREATE TABLE IF NOT EXISTS dialogue (
        id TEXT PRIMARY KEY NOT NULL,
        title TEXT NOT NULL DEFAULT '',
        token_count INTEGER NOT NULL DEFAULT 0,
        status INTEGER NOT NULL DEFAULT 0,
        is_pinned INTEGER NOT NULL DEFAULT 0,
        is_archived INTEGER NOT NULL DEFAULT 0,
        show_status INTEGER NOT NULL DEFAULT 1,
        system_prompt TEXT,
        message_count INTEGER NOT NULL DEFAULT 0,
        last_seq INTEGER NOT NULL DEFAULT 0,
        created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
        updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
    );";

/// 基础对话种子：与 `base_dialogue.rs` / `base_get_dialogue.rs` 的常量一致。
const SEED_SQL: [&str; 4] = [
    "INSERT OR REPLACE INTO dialogue
     (id, title, token_count, status, is_pinned, is_archived, show_status,
      system_prompt, message_count, last_seq)
     VALUES ('11111111-1111-4111-8111-111111111111', '产品需求讨论', 0, 0, 0, 0, 1, NULL, 0, 0)",
    "INSERT OR REPLACE INTO dialogue
     (id, title, token_count, status, is_pinned, is_archived, show_status,
      system_prompt, message_count, last_seq)
     VALUES ('22222222-2222-4222-8222-222222222222', '接口联调记录', 0, 0, 0, 0, 1, NULL, 0, 0)",
    "INSERT OR REPLACE INTO dialogue
     (id, title, token_count, status, is_pinned, is_archived, show_status,
      system_prompt, message_count, last_seq)
     VALUES ('33333333-3333-4333-8333-333333333333', '隐藏对话', 0, 0, 0, 0, 2, NULL, 0, 0)",
    "INSERT OR REPLACE INTO dialogue
     (id, title, token_count, status, is_pinned, is_archived, show_status,
      system_prompt, message_count, last_seq)
     VALUES ('44444444-4444-4444-8444-444444444444', '已归档对话', 0, 0, 1, 1, 1, NULL, 0, 0)",
];

fn prepare_dialogue_db() {
    chatllm::utils::db::db_update("dialogue", DIALOGUE_SCHEMA).expect("创建 dialogue 表失败");
    if let Err(error) = chatllm::utils::db::db_update(
        "dialogue",
        "ALTER TABLE dialogue ADD COLUMN system_prompt TEXT;",
    ) {
        println!("[tests] 补列跳过：{error}");
    }
    for sql in SEED_SQL {
        chatllm::utils::db::db_update("dialogue", sql).expect("写入测试种子失败");
    }
}

pub fn skip(reason: &str) {
    eprintln!("跳过 {reason}");
}

/// 调用一个 FFI 接口，打印请求与响应，返回解析后的响应 JSON。
pub fn call(entry: extern "C" fn(*const c_char, *mut c_char) -> c_int, body: &str) -> Value {
    let input = CString::new(body).expect("请求含 \\0");
    let mut out = [0 as c_char; 65536];
    let rc = entry(input.as_ptr(), out.as_mut_ptr());
    let out = unsafe { CStr::from_ptr(out.as_ptr()) }
        .to_string_lossy()
        .into_owned();
    println!("request : {}", body);
    println!("rc      : {}", rc);
    println!("response: {}", out);
    assert_eq!(rc, 0);
    serde_json::from_str(&out).expect("响应不是合法 JSON")
}
