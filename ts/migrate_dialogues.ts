// ts/migrate_dialogues.ts
// 迁移旧对话并清理旧库。
//
// 目标结构：
//   database/dialogue.db.dialogue         对话索引
//   designdb/<dialogue_id>/messages       每对话消息
//
// 旧结构：
//   database/dialogue.db                 索引（保留）
//   database/<uuid>.db                   旧每对话 SQLite 分库
//   designdb/chatllm                     过渡单库（dialogues / messages）
//
// 运行前提：
//   1. cyctron 已启动；
//   2. libary 中有 database.dll 与 libdesigndb.dll；
//   3. 在 Piksel 安装根目录执行。
//
// 用法：
//   deno run -A ts/migrate_dialogues.ts          # 迁移并删除旧库
//   deno run -A ts/migrate_dialogues.ts --keep   # 只迁移，保留旧库
//
// 说明：
//   - 已迁移的 seq 会跳过，可重复执行；
//   - 旧 SQLite 分库迁移完成后删除 `database/<uuid>.db`；
//   - 过渡单库迁移完成后删除 `designdb/chatllm/`。

const DATABASE_URL = "http://127.0.0.1:44944/database";
const DESIGNDB_URL = "http://127.0.0.1:44944/libdesigndb";
const DIALOGUE_DB = "dialogue";
const LEGACY_SINGLE = "chatllm";
const KEEP = Deno.args.includes("--keep");

function sqlStr(value: unknown): string {
  return `'${String(value ?? "").replace(/'/g, "''")}'`;
}

async function databaseQuery(path: string, sql: string): Promise<any[]> {
  const res = await fetch(DATABASE_URL, {
    method: "POST",
    headers: { "Content-Type": "application/libary", "FFI-Symbol": "db_query" },
    body: JSON.stringify({ path, sql }),
  });
  const data = await res.json() as any;
  if (!data || (data.code !== 200 && data.code !== 0)) {
    throw new Error(`SQLite 查询失败: ${JSON.stringify(data)}`);
  }
  const rows = data.data;
  if (Array.isArray(rows)) return rows;
  if (Array.isArray(rows?.list)) return rows.list;
  if (Array.isArray(rows?.rows)) return rows.rows;
  return [];
}

async function databaseUpdate(path: string, sql: string): Promise<void> {
  const res = await fetch(DATABASE_URL, {
    method: "POST",
    headers: { "Content-Type": "application/libary", "FFI-Symbol": "db_update" },
    body: JSON.stringify({ path, sql }),
  });
  const data = await res.json() as any;
  if (!data || (data.code !== 200 && data.code !== 0)) {
    throw new Error(`SQLite 写入失败: ${JSON.stringify(data)}`);
  }
}

async function designdbCall(symbol: string, body: unknown): Promise<any> {
  const res = await fetch(DESIGNDB_URL, {
    method: "POST",
    headers: { "Content-Type": "application/libary", "FFI-Symbol": symbol },
    body: JSON.stringify(body),
  });
  const data = await res.json() as any;
  if (!data || (data.code !== 200 && data.code !== 0)) {
    throw new Error(`designdb ${symbol} 失败: ${JSON.stringify(data)}`);
  }
  const value = data.data;
  if (Array.isArray(value)) return value;
  if (Array.isArray(value?.list)) return value.list;
  return value;
}

async function designdbQuery(path: string, collection: string, filter: Record<string, unknown> = {}) {
  const rows = await designdbCall("db_query", { path, collection, filter, limit: 100000 });
  return Array.isArray(rows) ? rows : [];
}

async function insertMessage(dialogueId: string, msg: {
  seq: number;
  role: string;
  content: string;
  created_at: number;
}) {
  await designdbCall("db_insert", {
    path: dialogueId,
    collection: "messages",
    doc: {
      _id: crypto.randomUUID(),
      dialogue_id: dialogueId,
      seq: msg.seq,
      role: msg.role,
      content: msg.content,
      created_at: msg.created_at,
    },
  });
}

/** SQLite 的 DATETIME 文本（UTC）或 Unix 秒 → 毫秒 */
function toMs(value: unknown): number {
  if (typeof value === "number") {
    return value > 1_000_000_000_000 ? value : value * 1000;
  }
  if (typeof value === "string" && value.length > 0) {
    const text = value.includes("T") ? value : value.replace(" ", "T") + "Z";
    const ms = Date.parse(text);
    if (!Number.isNaN(ms)) return ms;
  }
  return Date.now();
}

const stats = { dialogues: 0, messages: 0, skipped: 0, failed: 0 };

async function migrateSqliteShards() {
  const rows = await databaseQuery(DIALOGUE_DB, "SELECT id FROM dialogue");
  console.log(`索引对话数：${rows.length}`);

  for (const row of rows) {
    const id = String(row.id ?? "");
    if (!id) continue;

    let oldMessages: any[] = [];
    try {
      oldMessages = await databaseQuery(
        id,
        "SELECT id, role, content, created_at FROM messages ORDER BY id ASC",
      );
    } catch {
      // 没有旧分库
      continue;
    }
    if (oldMessages.length === 0) continue;

    try {
      const existing = await designdbQuery(id, "messages");
      const seen = new Set(existing.map((m) => Number(m.seq)));
      let lastSeq = existing.reduce((max, m) => Math.max(max, Number(m.seq) || 0), 0);
      let inserted = 0;

      for (const m of oldMessages) {
        const seq = Number(m.id ?? 0);
        if (seq <= 0 || seen.has(seq)) {
          stats.skipped += 1;
          continue;
        }
        await insertMessage(id, {
          seq,
          role: String(m.role ?? "user"),
          content: String(m.content ?? ""),
          created_at: toMs(m.created_at),
        });
        seen.add(seq);
        lastSeq = Math.max(lastSeq, seq);
        inserted += 1;
        stats.messages += 1;
      }

      if (inserted > 0) {
        const total = existing.length + inserted;
        await databaseUpdate(
          DIALOGUE_DB,
          `UPDATE dialogue SET message_count = ${total}, last_seq = ${lastSeq},
             updated_at = datetime('now') WHERE id = ${sqlStr(id)}`,
        );
      }

      stats.dialogues += 1;
      if (!KEEP && inserted > 0) {
        try {
          Deno.remove(`./database/${id}.db`);
          console.log(`已删除旧分库 database/${id}.db`);
        } catch (error) {
          console.warn(`删除 database/${id}.db 失败（可能仍被占用）: ${error}`);
        }
      }
    } catch (error) {
      stats.failed += 1;
      console.error(`迁移 ${id} 失败: ${error}`);
    }
  }
}

async function migrateLegacySingle() {
  let dialogues: any[] = [];
  try {
    dialogues = await designdbQuery(LEGACY_SINGLE, "dialogues");
  } catch {
    return;
  }
  if (dialogues.length === 0) return;

  console.log(`过渡单库对话数：${dialogues.length}`);
  for (const d of dialogues) {
    const id = String(d._id ?? "");
    if (!id) continue;
    try {
      await databaseUpdate(
        DIALOGUE_DB,
        `INSERT OR IGNORE INTO dialogue
         (id, title, keyword, model_name, system_prompt, model_config,
          parent_id, skills_json, context_json, token_count, status,
          is_pinned, is_archived, session_id, show_status, message_count, last_seq,
          created_at, updated_at)
         VALUES (${sqlStr(id)}, ${sqlStr(d.title)}, ${sqlStr(d.keyword)},
          ${sqlStr(d.model_name)}, ${d.system_prompt == null ? "NULL" : sqlStr(d.system_prompt)},
          NULL, NULL, '[]', NULL, 0, 0,
          ${Number(d.is_pinned ?? 0)}, ${Number(d.is_archived ?? 0)},
          '', 1, ${Number(d.message_count ?? 0)}, ${Number(d.last_seq ?? 0)},
          datetime('now'), datetime('now'))`,
      );

      const messages = await designdbQuery(LEGACY_SINGLE, "messages", { dialogue_id: id });
      const existing = await designdbQuery(id, "messages");
      const seen = new Set(existing.map((m) => Number(m.seq)));
      let inserted = 0;
      for (const m of messages) {
        const seq = Number(m.seq ?? 0);
        if (seq <= 0 || seen.has(seq)) continue;
        await insertMessage(id, {
          seq,
          role: String(m.role ?? "user"),
          content: String(m.content ?? ""),
          created_at: Number(m.created_at ?? Date.now()),
        });
        seen.add(seq);
        inserted += 1;
        stats.messages += 1;
      }
      if (inserted > 0) {
        const lastSeq = Math.max(...seen);
        await databaseUpdate(
          DIALOGUE_DB,
          `UPDATE dialogue SET message_count = ${seen.size}, last_seq = ${lastSeq},
             updated_at = datetime('now') WHERE id = ${sqlStr(id)}`,
        );
      }
      stats.dialogues += 1;
    } catch (error) {
      stats.failed += 1;
      console.error(`过渡单库对话 ${id} 迁移失败: ${error}`);
    }
  }

  if (!KEEP) {
    try {
      Deno.remove(`./designdb/${LEGACY_SINGLE}`, { recursive: true });
      console.log(`已删除过渡单库 designdb/${LEGACY_SINGLE}/`);
    } catch (error) {
      console.warn(`删除 designdb/${LEGACY_SINGLE}/ 失败: ${error}`);
    }
  }
}

async function main() {
  await migrateSqliteShards();
  await migrateLegacySingle();
  console.log(
    `迁移完成：对话 ${stats.dialogues}，消息 ${stats.messages}，跳过 ${stats.skipped}，失败 ${stats.failed}`,
  );
  if (stats.failed > 0) Deno.exitCode = 1;
}

await main();
