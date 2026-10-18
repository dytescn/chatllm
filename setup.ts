// ts/setup.ts —— 初始化 dialogue 库

// 1. PRAGMA 配置
await fetch("http://127.0.0.1:44944/database", {
  method: "POST",
  headers: {
    "Content-Type": "application/libary",
    "FFI-Symbol": "db_update",
  },
  body: JSON.stringify({ path: "./dialogue", sql: "PRAGMA foreign_keys = ON;" }),
});

await fetch("http://127.0.0.1:44944/database", {
  method: "POST",
  headers: {
    "Content-Type": "application/libary",
    "FFI-Symbol": "db_update",
  },
  body: JSON.stringify({ path: "./dialogue", sql: "PRAGMA journal_mode = WAL;" }),
});

await fetch("http://127.0.0.1:44944/database", {
  method: "POST",
  headers: {
    "Content-Type": "application/libary",
    "FFI-Symbol": "db_update",
  },
  body: JSON.stringify({ path: "./dialogue", sql: "PRAGMA cache_size = -64000;" }),
});

await fetch("http://127.0.0.1:44944/database", {
  method: "POST",
  headers: {
    "Content-Type": "application/libary",
    "FFI-Symbol": "db_update",
  },
  body: JSON.stringify({ path: "./dialogue", sql: "PRAGMA synchronous = NORMAL;" }),
});

// 2. 创建 prompts 表
await fetch("http://127.0.0.1:44944/database", {
  method: "POST",
  headers: {
    "Content-Type": "application/libary",
    "FFI-Symbol": "db_update",
  },
  body: JSON.stringify({
    path: "./dialogue",
    sql: `CREATE TABLE IF NOT EXISTS prompts (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        title TEXT NOT NULL,
        content TEXT NOT NULL,
        category TEXT DEFAULT 'General',
        is_system BOOLEAN DEFAULT 0,
        created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
        updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
    );`,
  }),
});

// 3. 创建 dialogue 表（对话列表）
await fetch("http://127.0.0.1:44944/database", {
  method: "POST",
  headers: {
    "Content-Type": "application/libary",
    "FFI-Symbol": "db_update",
  },
  body: JSON.stringify({
    path: "./dialogue",
    sql: `CREATE TABLE IF NOT EXISTS dialogue (
        id TEXT PRIMARY KEY NOT NULL,
        title TEXT NOT NULL DEFAULT '',
        keyword TEXT NOT NULL DEFAULT '',
        model_name TEXT NOT NULL DEFAULT '',
        system_prompt TEXT,
        model_config TEXT,
        is_pinned INTEGER NOT NULL DEFAULT 0,
        is_archived INTEGER NOT NULL DEFAULT 0,
        created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
        updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
    );`,
  }),
});

// 4. dialogue 表索引：归档 + 创建时间
await fetch("http://127.0.0.1:44944/database", {
  method: "POST",
  headers: {
    "Content-Type": "application/libary",
    "FFI-Symbol": "db_update",
  },
  body: JSON.stringify({
    path: "./dialogue",
    sql: `CREATE INDEX IF NOT EXISTS idx_dialogue_archived_created
        ON dialogue(is_archived, created_at DESC);`,
  }),
});

// 5. dialogue 表索引：置顶 + 创建时间
await fetch("http://127.0.0.1:44944/database", {
  method: "POST",
  headers: {
    "Content-Type": "application/libary",
    "FFI-Symbol": "db_update",
  },
  body: JSON.stringify({
    path: "./dialogue",
    sql: `CREATE INDEX IF NOT EXISTS idx_dialogue_pinned_created
        ON dialogue(is_pinned DESC, created_at DESC);`,
  }),
});

console.log("dialogue 初始化完成");

// ==================== session 表 ====================
await fetch("http://127.0.0.1:44944/database", {
  method: "POST",
  headers: { "Content-Type": "application/libary", "FFI-Symbol": "db_update" },
  body: JSON.stringify({
    path: "./dialogue",
    sql: `CREATE TABLE IF NOT EXISTS session (
        id TEXT PRIMARY KEY NOT NULL,
        dialogue_id TEXT NOT NULL DEFAULT '',
        parent_id TEXT,
        title TEXT NOT NULL DEFAULT '',
        system_prompt TEXT,
        model_name TEXT NOT NULL DEFAULT '',
        model_config TEXT,
        skills_json TEXT NOT NULL DEFAULT '[]',
        context_json TEXT,
        token_count INTEGER NOT NULL DEFAULT 0,
        status INTEGER NOT NULL DEFAULT 0,
        is_archived INTEGER NOT NULL DEFAULT 0,
        created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
        updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
    );`,
  }),
});

await fetch("http://127.0.0.1:44944/database", {
  method: "POST",
  headers: { "Content-Type": "application/libary", "FFI-Symbol": "db_update" },
  body: JSON.stringify({
    path: "./dialogue",
    sql: `CREATE INDEX IF NOT EXISTS idx_session_dialogue ON session(dialogue_id);`,
  }),
});

await fetch("http://127.0.0.1:44944/database", {
  method: "POST",
  headers: { "Content-Type": "application/libary", "FFI-Symbol": "db_update" },
  body: JSON.stringify({
    path: "./dialogue",
    sql: `CREATE INDEX IF NOT EXISTS idx_session_archived_created ON session(is_archived, created_at DESC);`,
  }),
});

// ==================== skills 表 ====================
await fetch("http://127.0.0.1:44944/database", {
  method: "POST",
  headers: { "Content-Type": "application/libary", "FFI-Symbol": "db_update" },
  body: JSON.stringify({
    path: "./dialogue",
    sql: `CREATE TABLE IF NOT EXISTS skills (
        id TEXT PRIMARY KEY NOT NULL,
        name TEXT NOT NULL DEFAULT '',
        description TEXT NOT NULL DEFAULT '',
        content TEXT NOT NULL DEFAULT '',
        category TEXT NOT NULL DEFAULT 'General',
        trigger TEXT,
        params_json TEXT,
        is_system INTEGER NOT NULL DEFAULT 0,
        is_enabled INTEGER NOT NULL DEFAULT 1,
        sort INTEGER NOT NULL DEFAULT 0,
        created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
        updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
    );`,
  }),
});

await fetch("http://127.0.0.1:44944/database", {
  method: "POST",
  headers: { "Content-Type": "application/libary", "FFI-Symbol": "db_update" },
  body: JSON.stringify({
    path: "./dialogue",
    sql: `CREATE INDEX IF NOT EXISTS idx_skills_category ON skills(category);`,
  }),
});

await fetch("http://127.0.0.1:44944/database", {
  method: "POST",
  headers: { "Content-Type": "application/libary", "FFI-Symbol": "db_update" },
  body: JSON.stringify({
    path: "./dialogue",
    sql: `CREATE INDEX IF NOT EXISTS idx_skills_enabled_sort ON skills(is_enabled, sort);`,
  }),
});

console.log("dialogue 初始化完成（含 session / skills）");
