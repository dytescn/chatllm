// ts/base_1.ts
// 基线测试 1：最小对话闭环
// 新建 → 续聊 → 列表 → 详情 → 搜索 → 更新 / 归档
//
// 前置：testenv 已启动（cyctron 监听 127.0.0.1:44944）。
// 运行：deno run --allow-net ts/base_1.ts
// 说明：chat 写入 user 消息后，会检索 chat_plugin 并调用插件，追加 assistant 结果；
//       响应的 content / content_html 仍为空字符串。

import { call, section, assert } from "./client.ts";

const stamp = Date.now().toString(36);
const title = `base_1-${stamp}`;
const keyword = `base_1-${stamp}`;

async function main() {
  // 1. 新建对话
  section("1. chat 新建");
  const created = await call("chat", {
    prompt: "base_1 基线测试",
    name: title,
    keyword,
  });
  console.log(created);
  assert(created.code === 200, "新建返回 200");
  assert(created.data?.created === true, "created 为 true");
  assert(created.data?.content === "", "content 为空（不调用模型）");
  assert(created.data?.content_html === "", "content_html 为空（不调用模型）");
  assert(created.data?.keyword === keyword, "keyword 回显正确");
  const uuid = created.data?.uuid;
  assert(typeof uuid === "string" && uuid.length === 36, "拿到合法 uuid");
  if (!uuid) return;

  // 2. 续聊
  section("2. chat 续聊");
  const continued = await call("chat", {
    prompt: "base_1 第二次写入",
    uuid,
  });
  console.log(continued);
  assert(continued.code === 200, "续聊返回 200");
  assert(continued.data?.created === false, "续聊 created 为 false");
  assert(continued.data?.uuid === uuid, "续聊 uuid 一致");

  // 3. 列表
  section("3. dialogue_list");
  const list = await call("dialogue_list", { page: 1, page_size: 100 });
  console.log(list);
  assert(list.code === 200, "列表返回 200");
  assert(Array.isArray(list.data?.list), "data.list 是数组");
  const row = (list.data?.list ?? []).find((r: any) => r.id === uuid);
  assert(!!row, "列表包含本对话");
  assert(row?.title === title, "title 正确");
  assert(row?.keyword === keyword, "keyword 正确");
  assert(row?.message_count === 4, "message_count = 4（每次 chat 写 user + assistant）");
  assert(row?.last_seq === 4, "last_seq = 4");

  // 4. 详情
  section("4. chat_detail");
  const detail = await call("chat_detail", { uuid, limit: 20 });
  console.log(detail);
  assert(detail.code === 200, "详情返回 200");
  assert((detail.data?.list ?? []).length === 4, "返回 4 条消息");
  assert(detail.data?.first_id === 1, "first_id = 1");
  assert(detail.data?.last_id === 4, "last_id = 4");
  assert(detail.data?.has_more_before === false, "没有更早消息");
  const messages = detail.data?.list ?? [];
  assert(messages[0]?.role === "user", "第 1 条是 user");
  assert(messages[0]?.seq === 1 && messages[0]?.id === 1, "第 1 条 seq/id = 1");
  assert(messages[0]?.content === "base_1 基线测试", "第 1 条内容正确");
  assert(messages[1]?.role === "assistant", "第 2 条是 assistant 结果");
  assert(messages[1]?.seq === 2 && messages[1]?.id === 2, "第 2 条 seq/id = 2");
  assert(typeof messages[1]?.content === "string" && messages[1].content.length > 0, "第 2 条内容非空");
  assert(!messages[1]?.content.includes("调用聊天插件失败"), "插件调用未失败");
  assert(messages[2]?.role === "user", "第 3 条是 user");
  assert(messages[2]?.seq === 3 && messages[2]?.id === 3, "第 3 条 seq/id = 3");
  assert(messages[2]?.content === "base_1 第二次写入", "第 3 条内容正确");
  assert(messages[3]?.role === "assistant", "第 4 条是 assistant 结果");
  assert(messages[3]?.seq === 4 && messages[3]?.id === 4, "第 4 条 seq/id = 4");
  assert(typeof messages[3]?.content === "string" && messages[3].content.length > 0, "第 4 条内容非空");
  assert(typeof messages[0]?.content_html === "string", "content_html 是字符串");
  assert(typeof messages[0]?.uuid === "string", "消息 uuid 存在");

  // 5. 搜索（按唯一 keyword 命中）
  section("5. dialogue_search");
  const searched = await call("dialogue_search", { keyword, page: 1, page_size: 20 });
  console.log(searched);
  assert(searched.code === 200, "搜索返回 200");
  assert(searched.data?.total === 1, "total = 1");
  assert((searched.data?.list ?? []).some((r: any) => r.id === uuid), "搜索命中本对话");

  // 6. 更新标题 + 归档
  section("6. dialogue_update：改标题并归档");
  const updated = await call("dialogue_update", {
    id: uuid,
    title: `${title}-updated`,
    is_archived: 1,
  });
  console.log(updated);
  assert(updated.code === 200, "更新返回 200");
  assert(updated.data?.id === uuid, "更新返回 id");

  // 7. 归档后列表与搜索都不可见
  section("7. 归档后不可见");
  const listAfter = await call("dialogue_list", { page: 1, page_size: 100 });
  assert(
    !(listAfter.data?.list ?? []).some((r: any) => r.id === uuid),
    "列表中不再出现已归档对话",
  );
  const searchAfter = await call("dialogue_search", { keyword, page: 1, page_size: 20 });
  assert(searchAfter.data?.total === 0, "搜索结果不再包含已归档对话");
}

main();
