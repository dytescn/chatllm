// runflow/base.ts
// 基础流程：
//   1. 发送信息并存储 user 消息
//   2. 判断 ai 配置：未配置时默认 deepseek
//   3. 检查对应 token：未配置直接报错；已配置则调用供应商并记录回复
//
// 前置：
//   1. testenv 已启动：cd testenv && ./cyctron.exe start
//   2. chat_plugin 中已安装 do_test_chat
//   3. testenv/libary/{libdeepseek,libsiliconflow_t2t}.dll 已安装
//   4. info.db：deepseek_app_key 未配置；silicon_app_key 已配置
// 运行：deno run --allow-net runflow/base.ts

const BASE = "http://127.0.0.1:44944";
const CHAT_PLUGIN_KEY = "chat-plugin-do-test-chat";

const CHAT_PLUGIN_ITEM = {
  key: CHAT_PLUGIN_KEY,
  value: JSON.stringify({
    name: "do_test_chat",
    description: "确定性的测试聊天插件，按 ai_supplier 转发",
    lib_name: "do_test_chat",
    ffi_symbol: "chat",
    category: "chat",
  }),
  value_type: "json",
  text: "确定性的测试聊天插件，按 ai_supplier 转发",
  rank: 1,
  meta: { plugin: "do_test_chat", category: "chat" },
};

async function post(api: string, symbol: string, body: unknown): Promise<any> {
  const res = await fetch(`${BASE}/${api}`, {
    method: "POST",
    headers: {
      "Content-Type": "application/libary",
      "FFI-Symbol": symbol,
    },
    body: JSON.stringify(body),
  });
  return await res.json();
}

let failed = 0;

function section(title: string) {
  console.log(`\n=== ${title} ===`);
}

function assert(cond: boolean, msg: string) {
  if (cond) {
    console.log(`✅ ${msg}`);
  } else {
    console.error(`❌ ${msg}`);
    failed += 1;
  }
}

function rows(resp: any): any[] {
  const data = resp?.data;
  if (Array.isArray(data)) return data;
  if (Array.isArray(data?.list)) return data.list;
  if (Array.isArray(data?.rows)) return data.rows;
  return [];
}

async function settings(): Promise<Record<string, string>> {
  const resp = await post("database", "db_query", {
    path: "./info",
    sql: "SELECT key, value FROM setting WHERE key IN ('ai_supplier','deepseek_app_key','silicon_app_key')",
  });
  const map: Record<string, string> = {};
  for (const row of rows(resp)) {
    if (typeof row.key === "string") map[row.key] = String(row.value ?? "");
  }
  return map;
}

async function clearAiSupplier() {
  return await post("database", "db_update", {
    path: "./info",
    sql: "DELETE FROM setting WHERE key = 'ai_supplier';",
  });
}

async function setAiSupplier(value: string) {
  return await post("database", "db_update", {
    path: "./info",
    sql: `INSERT INTO setting (key, value) VALUES ('ai_supplier', '${value}')
          ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = CURRENT_TIMESTAMP;`,
  });
}

async function chatAndReply(title: string, prompt: string) {
  const sent = await post("chatllm", "chat", { prompt, name: title });
  console.log(sent);
  assert(sent.code === 200, "chat 返回 200");
  const uuid = sent.data?.uuid;
  assert(typeof uuid === "string", "返回对话 uuid");
  if (!uuid) return { uuid: "", reply: "", user: "" };

  const detail = await post("chatllm", "chat_detail", { uuid, limit: 20 });
  console.log(detail);
  const messages = detail.data?.list ?? [];
  assert(detail.code === 200, "chat_detail 返回 200");
  assert(messages.length === 2, "写入 user + assistant 两条消息");
  assert(messages[0]?.role === "user", "第 1 条是 user");
  assert(messages[0]?.content === prompt, "user 内容已存储");
  assert(messages[1]?.role === "assistant", "第 2 条是 assistant");
  const reply = messages[1]?.content ?? "";
  console.log(`\n--- assistant ---\n${reply}\n`);
  return { uuid, reply, user: messages[0]?.content ?? "" };
}

async function main() {
  const stamp = Date.now().toString(36);

  // ------------------------------------------------------------------
  section("0. 准备：安装 do_test_chat，检查 AI 配置");
  const collection = await post("kv_lib", "kv_collection_put", {
    code: "chat_plugin",
    name: "聊天插件",
    description: "chat 分类插件的能力描述向量",
    dim: 1024,
    model: "qwen3-embed",
  });
  assert(collection.code === 200, "chat_plugin 集合可用");

  const put = await post("kv_lib", "kv_put", {
    collection: "chat_plugin",
    items: [CHAT_PLUGIN_ITEM],
  });
  assert(put.code === 200 && (put.data?.failed ?? 1) === 0, "安装 do_test_chat");

  const before = await settings();
  console.log(before);
  assert(!before.deepseek_app_key?.trim(), "前置：deepseek_app_key 未配置");
  assert(!!before.silicon_app_key?.trim(), "前置：silicon_app_key 已配置");

  // ------------------------------------------------------------------
  section("1. 未配置 ai 选项：默认 deepseek，缺 token 报错");
  const cleared = await clearAiSupplier();
  assert(cleared.code === 200, "清除 ai_supplier");
  const defaulted = await chatAndReply(`base-default-${stamp}`, "你好，默认用哪个模型？");
  assert(defaulted.reply.includes("AI 配置错误"), "assistant 报 AI 配置错误");
  assert(defaulted.reply.includes("deepseek"), "错误信息指向默认供应商 deepseek");
  assert(defaulted.reply.includes("deepseek_app_key"), "错误信息提示配置 deepseek_app_key");

  // ------------------------------------------------------------------
  section("2. 选择 siliconflow_t2t：token 已配置，发送并记录回复");
  const setSilicon = await setAiSupplier("siliconflow_t2t");
  assert(setSilicon.code === 200, "写入 ai_supplier = siliconflow_t2t");
  const silicon = await chatAndReply(`base-silicon-${stamp}`, "请用一句话介绍你自己");
  assert(silicon.reply.length > 0, "assistant 内容非空");
  assert(!silicon.reply.includes("AI 配置错误"), "不再报 AI 配置错误");
  assert(!silicon.reply.startsWith("[do_test_chat]"), "已真实调用供应商");

  // ------------------------------------------------------------------
  section("3. 显式选择 deepseek：缺 token 同样报错");
  const setDeepseek = await setAiSupplier("deepseek");
  assert(setDeepseek.code === 200, "写入 ai_supplier = deepseek");
  const deepseek = await chatAndReply(`base-deepseek-${stamp}`, "你好");
  assert(deepseek.reply.includes("AI 配置错误"), "assistant 报 AI 配置错误");
  assert(deepseek.reply.includes("deepseek_app_key"), "错误信息提示配置 deepseek_app_key");

  // ------------------------------------------------------------------
  section("4. 清理：恢复 ai_supplier，归档测试对话");
  const restored = await setAiSupplier("siliconflow_t2t");
  assert(restored.code === 200, "恢复 ai_supplier = siliconflow_t2t");
  for (const uuid of [defaulted.uuid, silicon.uuid, deepseek.uuid]) {
    const archived = await post("chatllm", "dialogue_update", { id: uuid, is_archived: 1 });
    assert(archived.code === 200, `归档 ${uuid}`);
  }

  console.log(`\n结果：${failed === 0 ? "全部通过" : `${failed} 项失败`}`);
  if (failed > 0) process.exitCode = 1;
}

main();
