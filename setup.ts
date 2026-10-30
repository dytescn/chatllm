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
    "FFI-Symbol": "db_query",
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
        parent_id TEXT,
        context_json TEXT,
        token_count INTEGER NOT NULL DEFAULT 0,
        status INTEGER NOT NULL DEFAULT 0,
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

// ==================== 旧表清理 ====================
async function tableExists(name: string): Promise<boolean> {
  const res = await fetch("http://127.0.0.1:44944/database", {
    method: "POST",
    headers: { "Content-Type": "application/libary", "FFI-Symbol": "db_query" },
    body: JSON.stringify({
      path: "./dialogue",
      sql: `SELECT name FROM sqlite_master WHERE type='table' AND name='${name}'`,
    }),
  });
  const data = await res.json() as any;
  const rows = data?.data;
  if (Array.isArray(rows)) return rows.length > 0;
  if (Array.isArray(rows?.list)) return rows.list.length > 0;
  return false;
}

async function dbUpdate(sql: string) {
  const res = await fetch("http://127.0.0.1:44944/database", {
    method: "POST",
    headers: { "Content-Type": "application/libary", "FFI-Symbol": "db_update" },
    body: JSON.stringify({ path: "./dialogue", sql }),
  });
  return res.json();
}

// 旧库补齐 session 字段（新库已在建表时包含）；已存在时 SQLite 会返回错误，忽略即可。
for (const [name, ddl] of [
  ["parent_id", "TEXT"],
  ["context_json", "TEXT"],
  ["token_count", "INTEGER NOT NULL DEFAULT 0"],
  ["status", "INTEGER NOT NULL DEFAULT 0"],
]) {
  await dbUpdate(`ALTER TABLE dialogue ADD COLUMN ${name} ${ddl}`);
}

// session 表已并入 dialogue：先按 dialogue_id 回填缺失字段，再删除旧表。
if (await tableExists("session")) {
  await dbUpdate(`UPDATE dialogue SET
    parent_id = COALESCE(parent_id, (SELECT s.parent_id FROM session s WHERE s.dialogue_id = dialogue.id)),
    context_json = COALESCE(context_json, (SELECT s.context_json FROM session s WHERE s.dialogue_id = dialogue.id)),
    token_count = CASE WHEN token_count = 0 THEN COALESCE((SELECT s.token_count FROM session s WHERE s.dialogue_id = dialogue.id), token_count) ELSE token_count END,
    status = CASE WHEN status = 0 THEN COALESCE((SELECT s.status FROM session s WHERE s.dialogue_id = dialogue.id), status) ELSE status END
    WHERE EXISTS (SELECT 1 FROM session s WHERE s.dialogue_id = dialogue.id);`);
  await dbUpdate("DROP TABLE session;");
  console.log("session 表已合并到 dialogue 并删除");
}

// 技能统一由 plugin_lib 管理，删除 chatllm 的旧 skills 表。
if (await tableExists("skills")) {
  await dbUpdate("DROP TABLE skills;");
  console.log("skills 表已删除");
}

// 旧对话结构：messages / messages_fts / sessions 表；消息已改存 designdb/<dialogue_id>。
for (const table of ["messages_fts", "messages", "sessions"]) {
  if (await tableExists(table)) {
    await dbUpdate(`DROP TABLE ${table};`);
    console.log(`${table} 表已删除`);
  }
}
