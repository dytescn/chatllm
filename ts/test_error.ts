// ts/test_error.ts
import { call, section, assert } from "./client.ts";

async function main() {
  // chat 非法 uuid
  section("chat：非法 uuid");
  const e1 = await call("chat", { prompt: "x", uuid: "not-a-uuid" });
  console.log(e1);
  assert(e1.code === 400103, "返回 400103");

  // chat 缺 prompt
  section("chat：缺 prompt");
  const e2 = await call("chat", {});
  console.log(e2);
  assert(e2.code === 400102, "返回 400102");

  // dialogue_update 非法 uuid
  section("dialogue_update：非法 uuid");
  const e3 = await call("dialogue_update", { id: "bad", title: "x" });
  console.log(e3);
  assert(e3.code === 400103, "返回 400103");

  // dialogue_update 缺 id
  section("dialogue_update：缺 id");
  const e4 = await call("dialogue_update", { title: "x" });
  console.log(e4);
  assert(e4.code === 400104, "返回 400104");

  // dialogue_search keyword 类型错
  section("dialogue_search：keyword 类型错");
  const e5 = await call("dialogue_search", { keyword: 123 });
  console.log(e5);
  assert(e5.code === 400105, "返回 400105");

  // dialogue_list page 类型错
  section("dialogue_list：page 类型错");
  const e6 = await call("dialogue_list", { page: "abc" });
  console.log(e6);
  assert(e6.code === 400106, "返回 400106");

  // dialogue_list page_size 为 0（clamp 到 1，不应报错）
  section("dialogue_list：page_size=0（clamp）");
  const e7 = await call("dialogue_list", { page: 1, page_size: 0 });
  console.log(e7);
  assert(e7.code === 200, "page_size=0 被 clamp 到 1，返回 200");
  assert(e7.data?.page_size === 1, "page_size 回显 1");
}

main();
