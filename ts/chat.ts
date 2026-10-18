// ts/chat.ts
import { call } from "./client.ts";

async function main() {
  console.log("=== chat 新建 ===");
  const c = await call("chat", {
    prompt: "你好，你是谁？",
    max_tokens: 128,
    name: "调试对话",
    keyword: "调试",
  });
  console.log(c);
  const uuid = c?.data?.uuid;
  if (!uuid) return;

  console.log("\n=== chat 续聊 ===");
  console.log(await call("chat", { prompt: "继续", uuid }));

  console.log("\n=== dialogue_list page=1 page_size=10 ===");
  console.log(await call("dialogue_list", { page: 1, page_size: 10 }));

  console.log("\n=== dialogue_search keyword=调试 ===");
  console.log(await call("dialogue_search", { keyword: "调试", page: 1, page_size: 10 }));
}

main();
