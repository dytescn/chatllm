// ts/test_cloud.ts
// 测试 cloud / 供应商配置的统一入口：
//   1. 未设置 ai_supplier → 设置提示 + /setting
//   2. 供应商已选但对应 AppKey 缺失 → 设置提示 + /setting
//   3. 供应商与 AppKey 完整 → route = null，进入 think → plugins → reply
//
// 前置：
//   1. testenv 已启动：cd testenv && ./cyctron.exe start
//   2. /chatllm、/kv_lib、/database 可调用
//   3. testenv 已部署 chatllm.dll、reply_sys_info
//
// 运行：
//   deno run --allow-net ts/test_cloud.ts
//
// 说明：
// - 测试用 AppKey 为临时值，不会访问真实供应商；
// - 完整流程使用本地 reply_sys_info 作为 chat_plugin，验证 cloud 前置校验
//   之后 plugins → reply 仍然工作；
// - 结束后恢复 ai_supplier、两个 AppKey、think-greeting，并归档测试对话。

const BASE = "http://127.0.0.1:44944";
const INFO_DB = "./info";
const SUPPLIER_KEY = "ai_supplier";
const DEEPSEEK_TOKEN = "deepseek_app_key";
const SILICON_TOKEN = "silicon_app_key";
const THINK_KEY = "think-greeting";
const PLUGIN_KEY = "chat-plugin-do-test-chat";
const THINK_COLLECTION = "chat_think";
const PLUGIN_COLLECTION = "chat_plugin";
const THINK_TEXT = "你好 问候 介绍 你是谁 在吗";

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
    throw new Error(`${api}/${symbol} 返回非 JSON（HTTP ${response.status}）：${text}`);
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

async function dbUpdate(sql: string): Promise<Json> {
  return await post("database", "db_update", { path: INFO_DB, sql });
}

async function dbQuery(sql: string): Promise<Json> {
  return await post("database", "db_query", { path: INFO_DB, sql });
}

async function ensureSettingTable(): Promise<void> {
  const result = await dbUpdate(
    "CREATE TABLE IF NOT EXISTS setting (key TEXT NOT NULL UNIQUE, value TEXT)",
  );
  if (result.code !== 200) {
    throw new Error(`创建 setting 表失败：${JSON.stringify(result)}`);
  }
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
  if (result.code !== 200) {
    throw new Error(`写入 ${key} 失败：${JSON.stringify(result)}`);
  }
}

async function deleteSetting(key: string): Promise<void> {
  await dbUpdate(`DELETE FROM setting WHERE key = '${esc(key)}'`);
}

async function restoreSetting(key: string, original: string | null): Promise<void> {
  if (original === null) {
    await deleteSetting(key);
  } else {
    await setSetting(key, original);
  }
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

async function ensurePluginCollection(): Promise<void> {
  const result = await post("kv_lib", "kv_collection", {
    action: "put",
    code: PLUGIN_COLLECTION,
    name: "聊天插件",
    description: "chat 分类插件的能力描述向量",
    dim: 1024,
    model: "qwen3-embed",
  });
  if (result.code !== 200) {
    throw new Error(`创建 ${PLUGIN_COLLECTION} 失败：${JSON.stringify(result)}`);
  }
}

function assistantContents(detail: Json): string[] {
  const list = detail?.data?.messages?.list ?? detail?.data?.list ?? [];
  return list
    .filter((message: Json) => message?.role === "assistant")
    .map((message: Json) => String(message?.content ?? ""));
}

async function chat(prompt: string, title: string): Promise<Json> {
  return await post("chatllm", "chat", { prompt, title });
}

async function detail(uuid: string): Promise<Json> {
  return await post("chatllm", "chat_detail", { uuid, limit: 20 });
}

async function assertBlocked(created: Json, label: string): Promise<string> {
  assert(created.code === 200, `${label}：chat 返回 200`);
  assert(created.data?.route === "/setting", `${label}：route = /setting`);
  const uuid = String(created.data?.uuid ?? "");
  assert(uuid.length === 36, `${label}：返回合法 uuid`);
  if (!uuid) return uuid;

  const res = await detail(uuid);
  const replies = assistantContents(res);
  assert(replies.length === 1, `${label}：写入 1 条 assistant 消息`);
  const reply = replies[0] ?? "";
  assert(reply.includes("请选择设置AI供应商"), `${label}：assistant 返回设置提示`);
  assert(reply.includes("AppKey"), `${label}：提示包含 AppKey`);
  assert(reply.includes("/setting"), `${label}：提示包含 /setting`);
  return uuid;
}

async function main() {
  const stamp = Date.now().toString(36);
  const uuids: string[] = [];

  const originalSupplier = await readSetting(SUPPLIER_KEY);
  const originalDeepseek = await readSetting(DEEPSEEK_TOKEN);
  const originalSilicon = await readSetting(SILICON_TOKEN);

  try {
    section("1. 未设置 ai_supplier");
    await deleteSetting(SUPPLIER_KEY);
    const unset = await chat("你好", `test_cloud-unset-${stamp}`);
    const unsetUuid = await assertBlocked(unset, "未设置供应商");
    if (unsetUuid) uuids.push(unsetUuid);

    section("2. deepseek 已选但 AppKey 缺失");
    await setSetting(SUPPLIER_KEY, "deepseek");
    await deleteSetting(DEEPSEEK_TOKEN);
    const missingDeepseek = await chat("你好", `test_cloud-deepseek-${stamp}`);
    const deepseekUuid = await assertBlocked(missingDeepseek, "deepseek 缺 AppKey");
    if (deepseekUuid) uuids.push(deepseekUuid);

    section("3. siliconflow_t2t 已选但 AppKey 缺失");
    await setSetting(SUPPLIER_KEY, "siliconflow_t2t");
    await deleteSetting(SILICON_TOKEN);
    const missingSilicon = await chat("你好", `test_cloud-silicon-${stamp}`);
    const siliconUuid = await assertBlocked(missingSilicon, "siliconflow 缺 AppKey");
    if (siliconUuid) uuids.push(siliconUuid);

    section("4. 供应商与 AppKey 完整：进入 think → plugins → reply");
    await setSetting(SUPPLIER_KEY, "deepseek");
    await setSetting(DEEPSEEK_TOKEN, "test-token");

    const originalThink = await kvGet(THINK_COLLECTION, [THINK_KEY]);
    await kvPut(THINK_COLLECTION, {
      key: THINK_KEY,
      value: "不应使用的片段|测试查询词",
      value_type: "text",
      text: originalThink?.text ?? THINK_TEXT,
      rank: originalThink?.rank ?? 1,
      meta: originalThink?.meta ?? { category: "chat_think" },
    });

    await ensurePluginCollection();
    await kvPut(PLUGIN_COLLECTION, {
      key: PLUGIN_KEY,
      value: JSON.stringify({
        name: "reply_sys_info",
        lib_name: "reply_sys_info",
        ffi_symbol: "chat",
        category: "chat",
      }),
      value_type: "json",
      text: "测试查询词",
      rank: 1,
      meta: { plugin: "reply_sys_info", category: "chat" },
    });

    const ready = await chat("你好", `test_cloud-ready-${stamp}`);
    console.log(ready);
    assert(ready.code === 200, "完整配置：chat 返回 200");
    assert(ready.data?.route === null, "完整配置：route = null");
    const readyUuid = String(ready.data?.uuid ?? "");
    assert(readyUuid.length === 36, "完整配置：返回合法 uuid");

    if (readyUuid) {
      uuids.push(readyUuid);
      const res = await detail(readyUuid);
      const replies = assistantContents(res);
      const reply = replies[0] ?? "";
      assert(!reply.includes("请选择设置AI供应商"), "完整配置：没有返回设置提示");
      assert(reply.includes("我收到你的信息了"), "完整配置：plugins 结果已交给 reply");
    }
  } finally {
    section("5. 清理：恢复配置并归档测试对话");

    try {
      await restoreSetting(DEEPSEEK_TOKEN, originalDeepseek);
      await restoreSetting(SILICON_TOKEN, originalSilicon);
      await restoreSetting(SUPPLIER_KEY, originalSupplier);
      assert((await readSetting(SUPPLIER_KEY)) === originalSupplier, "ai_supplier 已恢复");
      assert((await readSetting(DEEPSEEK_TOKEN)) === originalDeepseek, "deepseek_app_key 已恢复");
      assert((await readSetting(SILICON_TOKEN)) === originalSilicon, "silicon_app_key 已恢复");
    } catch (error) {
      console.error("恢复配置失败：", error);
      failed += 1;
    }

    for (const uuid of uuids) {
      const archived = await post("chatllm", "dialogue_update", {
        id: uuid,
        is_archived: 1,
      });
      assert(archived.code === 200, `归档测试对话 ${uuid.slice(0, 8)}`);
    }
  }

  console.log(`\n结果：${failed === 0 ? "全部通过" : `${failed} 项失败`}`);
  if (failed > 0) Deno.exit(1);
}

main().catch((error) => {
  console.error("测试执行失败：", error);
  Deno.exit(1);
});
