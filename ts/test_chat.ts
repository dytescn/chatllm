// ts/test_chat.ts
import { call, section, assert } from "./client.ts";

async function main() {
  // 1. 新建：带 name + keyword
  section("chat 新建：带 name + keyword");
  const c1 = await call("chat", {
    prompt: "你好，你是谁？",
    max_tokens: 128,
    name: "测试对话A",
    keyword: "测试,你好",
  });
  console.log(c1);
  assert(c1.code === 200, "新建返回 200");
  assert(c1.data?.created === true, "created 为 true");
  assert(typeof c1.data?.content === "string" && c1.data.content.length > 0, "content 非空");
  const uuidA = c1.data?.uuid;
  assert(typeof uuidA === "string" && uuidA.length === 36, "拿到合法 uuid");
  if (!uuidA) return;

  // 2. 新建：无 name 无 keyword
  section("chat 新建：无 name 无 keyword");
  const c2 = await call("chat", { prompt: "今天天气怎么样" });
  console.log(c2);
  assert(c2.code === 200 && c2.data?.created === true, "第二条新建成功");
  assert(typeof c2.data?.content === "string" && c2.data.content.length > 0, "content 非空");
  const uuidB = c2.data?.uuid;

  // 3. 续聊：带 keyword
  section("chat 续聊：带 keyword");
  const r1 = await call("chat", { prompt: "继续说说", uuid: uuidA, keyword: "重要客户" });
  console.log(r1);
  assert(r1.code === 200 && r1.data?.created === false, "续聊 created 为 false");
  assert(r1.data?.uuid === uuidA, "uuid 一致");
  assert(typeof r1.data?.content === "string" && r1.data.content.length > 0, "续聊 content 非空");

  // 4. 续聊：不带 keyword
  section("chat 续聊：不带 keyword");
  const r2 = await call("chat", { prompt: "还有呢？", uuid: uuidA });
  console.log(r2);
  assert(r2.code === 200, "续聊无 keyword 成功");

  // 5. 列表（分页）：校验 A 的 keyword
  section("dialogue_list 分页：校验 A");
  const list = await call("dialogue_list", { page: 1, page_size: 50 });
  console.log(list);
  assert(list.code === 200, "list 返回 200");
  assert(Array.isArray(list.data?.list), "data.list 是数组");
  assert(typeof list.data?.total === "number", "data.total 是数字");
  const rowA = (list.data?.list ?? []).find((r: any) => r.id === uuidA);
  assert(!!rowA, "列表含 A");
  assert(rowA?.keyword === "重要客户", "A 的 keyword 已更新为 重要客户");
  assert(!!uuidB, "B uuid 存在");

  // 6. 列表分页边界：page_size=1
  section("dialogue_list page_size=1");
  const p1 = await call("dialogue_list", { page: 1, page_size: 1 });
  console.log(p1);
  assert(p1.code === 200, "page_size=1 返回 200");
  assert((p1.data?.list ?? []).length <= 1, "单页最多 1 条");
  assert(p1.data?.page === 1 && p1.data?.page_size === 1, "分页参数回显正确");

  // 7. 列表：超大 page_size 被 clamp 到 100
  section("dialogue_list page_size=9999（clamp 100）");
  const p2 = await call("dialogue_list", { page: 1, page_size: 9999 });
  console.log(p2);
  assert(p2.code === 200, "超大 page_size 返回 200");
  assert(p2.data?.page_size === 100, "page_size 被 clamp 到 100");

  // 8. 列表：page 超出范围返回空 list
  section("dialogue_list page=99999");
  const p3 = await call("dialogue_list", { page: 99999, page_size: 20 });
  console.log(p3);
  assert(p3.code === 200, "超范围 page 返回 200");
  assert((p3.data?.list ?? []).length === 0, "超范围 page 返回空 list");
  assert(typeof p3.data?.total === "number", "total 仍返回");
}

main();
