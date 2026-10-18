// ts/test_search.ts
import { call, section, assert } from "./client.ts";

async function main() {
  // 准备两条，keyword 可区分
  section("准备数据");
  const a = await call("chat", {
    prompt: "苹果怎么样",
    name: "水果讨论",
    keyword: "苹果,水果",
  });
  const b = await call("chat", {
    prompt: "香蕉怎么样",
    name: "另一种水果",
    keyword: "香蕉",
  });
  console.log(a, b);
  const idA = a.data?.uuid;
  const idB = b.data?.uuid;

  // 按 keyword 命中 A
  section("search keyword=苹果");
  const s1 = await call("dialogue_search", { keyword: "苹果" });
  console.log(s1);
  assert(s1.code === 200, "search 返回 200");
  assert(Array.isArray(s1.data?.list), "data.list 是数组");
  assert((s1.data?.list ?? []).some((r: any) => r.id === idA), "命中 A");
  assert(typeof s1.data?.total === "number", "total 是数字");

  // 按 title 命中 B
  section("search keyword=另一种");
  const s2 = await call("dialogue_search", { keyword: "另一种" });
  console.log(s2);
  assert((s2.data?.list ?? []).some((r: any) => r.id === idB), "命中 B（title）");

  // 不命中
  section("search keyword=zzz不存在");
  const s3 = await call("dialogue_search", { keyword: "zzz不存在" });
  console.log(s3);
  assert((s3.data?.list ?? []).length === 0, "不命中返回空 list");
  assert(s3.data?.total === 0, "total 为 0");

  // 空 keyword → 全部（分页）
  section("search keyword=''（分页）");
  const s4 = await call("dialogue_search", { keyword: "", page: 1, page_size: 20 });
  console.log(s4);
  assert(s4.code === 200, "空 keyword 返回 200");
  assert(Array.isArray(s4.data?.list), "空 keyword list 是数组");
  assert((s4.data?.list ?? []).length <= 20, "单页不超过 page_size");
  assert((s4.data?.total ?? 0) >= 2, "total 至少 2");

  // 不传 keyword → 全部
  section("search 不传 keyword");
  const s5 = await call("dialogue_search", {});
  console.log(s5);
  assert(s5.code === 200, "无 keyword 返回 200");
  assert(Array.isArray(s5.data?.list), "无 keyword list 是数组");

  // 搜索分页：page_size=1
  section("search 分页 page_size=1");
  const s6 = await call("dialogue_search", { keyword: "", page: 1, page_size: 1 });
  console.log(s6);
  assert((s6.data?.list ?? []).length <= 1, "单页最多 1 条");
  assert(s6.data?.page_size === 1, "page_size 回显 1");
}

main();
