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
        session_id TEXT NOT NULL DEFAULT '',
        show_status INTEGER NOT NULL DEFAULT 1,
        message_count INTEGER NOT NULL DEFAULT 0,
        last_seq INTEGER NOT NULL DEFAULT 0,
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

// ==================== embedding 分类 / 分片元数据 ====================
// 向量数据在 database/embedding_<category>_<n>.db 的 prompts_embedding 表；
// 这里只建分类与分片注册表。
await fetch("http://127.0.0.1:44944/database", {
  method: "POST",
  headers: { "Content-Type": "application/libary", "FFI-Symbol": "db_update" },
  body: JSON.stringify({
    path: "./dialogue",
    sql: `CREATE TABLE IF NOT EXISTS embedding_category (
        code TEXT PRIMARY KEY NOT NULL,
        name TEXT NOT NULL DEFAULT '',
        description TEXT NOT NULL DEFAULT '',
        sort INTEGER NOT NULL DEFAULT 0,
        created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
    );`,
  }),
});

await fetch("http://127.0.0.1:44944/database", {
  method: "POST",
  headers: { "Content-Type": "application/libary", "FFI-Symbol": "db_update" },
  body: JSON.stringify({
    path: "./dialogue",
    sql: `CREATE TABLE IF NOT EXISTS embedding_shard (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        category TEXT NOT NULL,
        shard_no INTEGER NOT NULL,
        path TEXT NOT NULL UNIQUE,
        row_count INTEGER NOT NULL DEFAULT 0,
        size_bytes INTEGER NOT NULL DEFAULT 0,
        state INTEGER NOT NULL DEFAULT 0,
        created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
        updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
        UNIQUE(category, shard_no)
    );`,
  }),
});

await fetch("http://127.0.0.1:44944/database", {
  method: "POST",
  headers: { "Content-Type": "application/libary", "FFI-Symbol": "db_update" },
  body: JSON.stringify({
    path: "./dialogue",
    sql: `CREATE INDEX IF NOT EXISTS idx_embedding_shard_active
        ON embedding_shard(category, state);`,
  }),
});

await fetch("http://127.0.0.1:44944/database", {
  method: "POST",
  headers: { "Content-Type": "application/libary", "FFI-Symbol": "db_update" },
  body: JSON.stringify({
    path: "./dialogue",
    sql: `INSERT OR IGNORE INTO embedding_category (code, name, sort)
        VALUES ('plugin', '插件能力', 10);`,
  }),
});

await fetch("http://127.0.0.1:44944/database", {
  method: "POST",
  headers: { "Content-Type": "application/libary", "FFI-Symbol": "db_update" },
  body: JSON.stringify({
    path: "./dialogue",
    sql: `INSERT OR IGNORE INTO embedding_category (code, name, sort)
        VALUES ('prompt', '提示词', 20);`,
  }),
});

await fetch("http://127.0.0.1:44944/database", {
  method: "POST",
  headers: { "Content-Type": "application/libary", "FFI-Symbol": "db_update" },
  body: JSON.stringify({
    path: "./dialogue",
    sql: `INSERT OR IGNORE INTO embedding_category (code, name, sort)
        VALUES ('skill', '技能', 30);`,
  }),
});

await fetch("http://127.0.0.1:44944/database", {
  method: "POST",
  headers: { "Content-Type": "application/libary", "FFI-Symbol": "db_update" },
  body: JSON.stringify({
    path: "./dialogue",
    sql: `INSERT OR IGNORE INTO embedding_category (code, name, sort)
        VALUES ('material', '素材', 40);`,
  }),
});

console.log("embedding 分类 / 分片元数据初始化完成");

// ==================== designdb（对话数据） ====================
// 对话与消息已迁到 designdb；上面 SQLite 表只保留 session / skills /
// prompts / embedding 分类与分片元数据，直到后续迁移完成。
const DESIGNDB_URL = "http://127.0.0.1:44944/libdesigndb";

async function designdbCall(symbol: string, body: unknown) {
  const res = await fetch(DESIGNDB_URL, {
    method: "POST",
    headers: {
      "Content-Type": "application/libary",
      "FFI-Symbol": symbol,
    },
    body: JSON.stringify(body),
  });
  const data = await res.json().catch(() => null) as any;
  if (!data || (data.code !== 200 && data.code !== 0)) {
    console.warn(`[designdb] ${symbol} 失败:`, data);
  }
  return data;
}

const meta = await designdbCall("db_query", {
  path: "chatllm",
  collection: "meta",
  filter: { _id: "schema" },
  limit: 1,
});
if (!Array.isArray(meta?.data) || meta.data.length === 0) {
  await designdbCall("db_insert", {
    path: "chatllm",
    collection: "meta",
    doc: {
      _id: "schema",
      version: 2,
      engine: "designdb",
      created_at: Date.now(),
      updated_at: Date.now(),
    },
  });
}

await designdbCall("db_create_index", {
  path: "chatllm",
  collection: "dialogues",
  index: {
    key: { is_archived: 1, is_pinned: -1, created_at: -1 },
    name: "idx_dialogues_list",
  },
});

await designdbCall("db_create_index", {
  path: "chatllm",
  collection: "messages",
  index: {
    key: { dialogue_id: 1, seq: 1 },
    name: "idx_messages_dialogue_seq",
    unique: true,
  },
});

console.log("designdb 初始化完成（chatllm / meta + 索引）");
