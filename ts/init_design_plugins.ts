// ==================== init_design_plugins 表 ====================
const res = await fetch("http://127.0.0.1:44944/database", {
  method: "POST",
  headers: { "Content-Type": "application/libary", "FFI-Symbol": "db_update" },
  body: JSON.stringify({
    path: "./design_plugins",
    sql: `CREATE TABLE IF NOT EXISTS design_plugins (
        id TEXT PRIMARY KEY NOT NULL,
        uuid TEXT UNIQUE,
        name TEXT,
        path TEXT,
        ffi TEXT,
        soft TEXT,
        description TEXT,
        is_enabled INTEGER NOT NULL DEFAULT 1,
        is_local INTEGER NOT NULL DEFAULT 1,
        sort INTEGER NOT NULL DEFAULT 0,
        created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
        updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
    );`,
  }),
});
console.log("create:", await res.json());



// ==================== init_prompts 表 ====================
await fetch("http://127.0.0.1:44944/database", {
  method: "POST",
  headers: { "Content-Type": "application/libary", "FFI-Symbol": "db_update" },
  body: JSON.stringify({
    path: "./design_plugins",
    sql:`CREATE TABLE IF NOT EXISTS prompts (
    id TEXT PRIMARY KEY NOT NULL,
    uuid TEXT NOT NULL,
    embedding BLOB NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_prompts_uuid ON prompts(uuid);
    );`,
  }),
});
