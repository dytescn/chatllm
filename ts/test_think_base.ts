// ts/test_think_base.ts
// 调试 think_base 阶段：
//   发送消息 → think 查 chat_think → 取 think_base uuid
//   → 查询 / 创建对话 → skills 写入 system_prompt → 写入 user 消息
//
// 前置：
//   1. testenv 已启动：cd testenv && ./cyctron.exe start
//   2. testenv/plugins/think_base/ 已安装
//   3. dialogue 表已包含 system_prompt 列（setup.ts）
//
// 运行：
//   deno run --allow-net --allow-read ts/test_think_base.ts

const BASE = "http://127.0.0.1:44944";
const INFO_DB = "./info";
const DIALOGUE_DB = "./dialogue";
const THINK_COLLECTION = "chat_think";
const PLUGIN_COLLECTION = "chat_plugin";
const THINK_KEY = "think-base";
const PLUGIN_KEY = "chat-plugin-do-test-chat";
const SUPPLIER_KEY = "ai_supplier";
const TOKEN_KEY = "deepseek_app_key";

type Json = Record<string, any>;

let failed = 0;

function section(title: string) {
  console.log(`\n=== ${title} ===`);
}

function assert(cond: boolean, message: string) {
  if (cond) {
    console.log(`✅ ${message}`);
  } else {
    console.error(`❌ ${message}`);
    failed += 1;
  }
}

async function post(api: string, symbol: string, body: Json = {}): Promise<Json> {
  const response = await fetch(`${BASE}/${api}`, {
    method: "POST",
    headers: {
      "Content-Type": "application/libary",
      "FFI-Symbol": symbol,
    },
    body: JSON.stringify(body),
  });
  const text = await response.text();
  try {
    return JSON.parse(text);
  } catch {
    throw new Error(`${api}/${symbol} 返回非 JSON（HTTP ${response.status}）`);
  }
}

function rows(payload: Json): Json[] {
  const data = payload?.data;
  if (Array.isArray(data)) return data;
  if (Array.isArray(data?.list)) return data.list;
  if (Array.isArray(data?.rows)) return data.rows;
  return [];
}

function esc(value: string): string {
  return value.replace(/'/g, "''");
}

async function dbQuery(sql: string): Promise<Json> {
  return await post("database", "db_query", { path: INFO_DB, sql });
}

async function dbUpdate(sql: string): Promise<Json> {
  return await post("database", "db_update", { path: INFO_DB, sql });
}

async function dialogueUpdate(sql: string): Promise<Json> {
  return await post("database", "db_update", { path: DIALOGUE_DB, sql });
}

async function ensureSettingTable(): Promise<void> {
  const result = await dbUpdate(
    "CREATE TABLE IF NOT EXISTS setting (key TEXT NOT NULL UNIQUE, value TEXT)",
  );
  if (result.code !== 200) throw new Error("创建 setting 表失败");
}

async function readSetting(key: string): Promise<string | null> {
  const result = await dbQuery(
    `SELECT value FROM setting WHERE key = '${esc(key)}' LIMIT 1`,
  );
  if (result.code !== 200) return null;
  const value = rows(result)[0]?.value;
  return typeof value === "string" ? value : null;
}

async function setSetting(key: string, value: string): Promise<void> {
  await ensureSettingTable();
  const result = await dbUpdate(
    `INSERT INTO setting (key, value) VALUES ('${esc(key)}', '${esc(value)}')
     ON CONFLICT(key) DO UPDATE SET value = excluded.value`,
  );
  if (result.code !== 200) throw new Error(`写入 ${key} 失败`);
}

async function deleteSetting(key: string): Promise<void> {
  await dbUpdate(`DELETE FROM setting WHERE key = '${esc(key)}'`);
}

async function restoreSetting(key: string, original: string | null): Promise<void> {
  if (original === null) await deleteSetting(key);
  else await setSetting(key, original);
}

async function kvGet(collection: string, keys: string[]): Promise<Json | null> {
  const result = await post("kv_lib", "kv_record", {
    action: "get",
    collection,
    keys,
    with_vector: false,
  });
  if (result.code !== 200) return null;
  return rows(result)[0] ?? null;
}

async function kvPut(collection: string, item: Json): Promise<void> {
  const result = await post("kv_lib", "kv_record", {
    action: "put",
    collection,
    items: [item],
  });
  if (result.code !== 200) {
    throw new Error(`写入 ${collection}/${item.key} 失败：${JSON.stringify(result)}`);
  }
}

async function ensureCollection(code: string, name: string, description: string): Promise<void> {
  const result = await post("kv_lib", "kv_collection", {
    action: "put",
    code,
    name,
    description,
    dim: 1024,
    model: "qwen3-embed",
  });
  if (result.code !== 200) throw new Error(`创建 ${code} 失败`);
}

function assistantContents(detail: Json): string[] {
  const list = detail?.data?.messages?.list ?? [];
  return list
    .filter((message: Json) => message?.role === "assistant")
    .map((message: Json) => String(message?.content ?? ""));
}

function userContents(detail: Json): string[] {
  const list = detail?.data?.messages?.list ?? [];
  return list
    .filter((message: Json) => message?.role === "user")
    .map((message: Json) => String(message?.content ?? ""));
}

async function main() {
  const packageDir = "testenv/plugins/think_base";
  const manifest = JSON.parse(await Deno.readTextFile(`${packageDir}/dytes.json`));
  const thinkBaseUuid = String(manifest.uuid);
  const prompts = JSON.parse(await Deno.readTextFile(`${packageDir}/think_base.json`));
  const skills = await Deno.readTextFile(`${packageDir}/think_base.md`);
  const promptText = ["think_base", "测试", ...prompts].join(" ");

  const originalSupplier = await readSetting(SUPPLIER_KEY);
  const originalToken = await readSetting(TOKEN_KEY);
  const originalThink = await kvGet(THINK_COLLECTION, [THINK_KEY]);
  const originalPlugin = await kvGet(PLUGIN_COLLECTION, [PLUGIN_KEY]);

  let uuid = "";

  try {
    section("1. 准备 think_base 的 chat_think 记录");
    await setSetting(SUPPLIER_KEY, "deepseek");
    await setSetting(TOKEN_KEY, "test-token");
    await ensureCollection("chat_think", "思考", "think 阶段分类");
    await kvPut(THINK_COLLECTION, {
      key: THINK_KEY,
      value: "问候|系统信息",
      value_type: "text",
      text: promptText,
      rank: 1,
      meta: {
        category: "chat_think",
        plugin_uuid: thinkBaseUuid,
        plugin_name: "think_base",
      },
    });
    assert(true, `chat_think 已写入 think_base（${thinkBaseUuid.slice(0, 8)}）`);

    // 为拆分后的每一段准备一个可执行的插件记录。
    await ensureCollection(PLUGIN_COLLECTION, "聊天插件", "chat 分类插件");
    for (const [key, text] of [
      ["chat-plugin-think-greeting", "问候"],
      ["chat-plugin-think-sysinfo", "系统信息"],
    ]) {
      await kvPut(PLUGIN_COLLECTION, {
        key,
        value: JSON.stringify({
          name: "reply_sys_info",
          lib_name: "reply_sys_info",
          ffi_symbol: "chat",
          category: "chat",
        }),
        value_type: "json",
        text,
        rank: 1,
        meta: { plugin: "reply_sys_info", category: "chat" },
      });
    }
    assert(true, "chat_plugin 已准备“问候”“系统信息”两条记录");

    // 避免复用之前测试遗留的同 id 对话
    await dialogueUpdate(`DELETE FROM dialogue WHERE id = '${esc(thinkBaseUuid)}'`);

    section("2. 第一条消息：think 找到 think_base 并创建对话");
    const created = await post("chatllm", "chat", { prompt: "think_base 测试" });
    console.log(created);
    assert(created.code === 200, "chat 返回 200");
    assert(created.data?.uuid === thinkBaseUuid, "对话 id = think_base uuid");
    assert(created.data?.created === true, "created = true");
    assert(created.data?.plugin_uuid === thinkBaseUuid, "plugin_uuid = think_base uuid");
    uuid = String(created.data?.uuid ?? "");

    const detail = await post("chatllm", "chat_detail", { uuid, limit: 20 });
    console.log(detail);
    assert(detail.code === 200, "chat_detail 返回 200");
    assert(
      detail.data?.dialogue?.system_prompt === skills.trim(),
      "dialogue.system_prompt = think_base skills",
    );
    assert(
      userContents(detail).includes("think_base 测试"),
      "写入 user 消息",
    );

    section("3. 第二条消息：复用已有对话");
    const continued = await post("chatllm", "chat", { prompt: "think_base 继续" });
    console.log(continued);
    assert(continued.code === 200, "续聊返回 200");
    assert(continued.data?.uuid === thinkBaseUuid, "续聊仍使用 think_base uuid");
    assert(continued.data?.created === false, "created = false");

    const detail2 = await post("chatllm", "chat_detail", { uuid, limit: 20 });
    assert(
      detail2.data?.dialogue?.system_prompt === skills.trim(),
      "system_prompt 未变化",
    );
    assert(
      userContents(detail2).includes("think_base 继续"),
      "第二条 user 消息已写入",
    );

    section("4. 当前阶段结果");
    const assistant = assistantContents(detail2);
    const lastAssistant = assistant[assistant.length - 1] ?? "";
    console.log(`assistant messages: ${assistant.length}`);
    console.log(`user messages: ${userContents(detail2).length}`);
    console.log(`last assistant:\n${lastAssistant}`);
    assert(lastAssistant.includes("问候"), "plugins 结果包含“问候”段");
    assert(lastAssistant.includes("系统信息"), "plugins 结果包含“系统信息”段");
  } finally {
    section("5. 清理");
    try {
      await restoreSetting(TOKEN_KEY, originalToken);
      await restoreSetting(SUPPLIER_KEY, originalSupplier);
      assert(true, "配置已恢复");
    } catch (error) {
      console.error("恢复配置失败：", error);
      failed += 1;
    }

    if (originalThink) {
      try {
        await kvPut(THINK_COLLECTION, {
          key: THINK_KEY,
          value: originalThink.value ?? "",
          value_type: originalThink.value_type ?? "text",
          text: originalThink.text ?? "",
          rank: originalThink.rank ?? 1,
          meta: originalThink.meta ?? { category: "chat_think" },
        });
      } catch (error) {
        console.error("恢复 think-base 失败：", error);
        failed += 1;
      }
    }

    if (originalPlugin) {
      try {
        await kvPut(PLUGIN_COLLECTION, {
          key: PLUGIN_KEY,
          value: originalPlugin.value ?? "",
          value_type: originalPlugin.value_type ?? "json",
          text: originalPlugin.text ?? "测试查询词",
          rank: originalPlugin.rank ?? 1,
          meta: originalPlugin.meta ?? { category: "chat" },
        });
      } catch (error) {
        console.error("恢复 chat_plugin 失败：", error);
        failed += 1;
      }
    }

    if (uuid) {
      await dialogueUpdate(`DELETE FROM dialogue WHERE id = '${esc(uuid)}'`);
      assert(true, "测试对话已删除");
    }
  }

  console.log(`\n结果：${failed === 0 ? "全部通过" : `${failed} 项失败`}`);
  if (failed > 0) Deno.exit(1);
}

main().catch((error) => {
  console.error("测试执行失败：", error);
  Deno.exit(1);
});
