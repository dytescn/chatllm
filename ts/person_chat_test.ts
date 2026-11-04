// ts/person_chat_test.ts
// 交互式对话测试：输入文字 → 得到回复，支持连续提问（同一对话内多轮）。
//
// 运行：
//   deno run --allow-ffi --allow-env --allow-read ts/person_chat_test.ts
//
// 命令：
//   /new      开始新对话（之后的提问作为新会话）
//   /history  打印当前对话的全部消息
//   /exit     退出（等同 Ctrl+C / Ctrl+D）
//
// 说明：直接调用 testenv/libary/chatllm.dll 的 FFI，
// 工作目录切到 testenv，和 cargo test / 其他 TS 测试保持一致。
// think / plugins / reply / reply_base 阶段的日志会打印在终端。

const OUT_SIZE = 65_536;

const encoder = new TextEncoder();
const decoder = new TextDecoder();

function toWindowsPath(url: URL): string {
  let path = decodeURIComponent(url.pathname);
  if (Deno.build.os === "windows") {
    if (/^\/[A-Za-z]:/.test(path)) path = path.slice(1);
    path = path.replaceAll("/", "\\");
  }
  return path;
}

const testenvDir = toWindowsPath(new URL("../testenv/", import.meta.url));
const libDir = `${testenvDir}libary`;

const currentPath = Deno.env.get("PATH") ?? "";
const separator = Deno.build.os === "windows" ? ";" : ":";
if (!currentPath.split(separator).includes(libDir)) {
  Deno.env.set("PATH", `${libDir}${separator}${currentPath}`);
}
Deno.chdir(testenvDir);

const SYMBOLS = { parameters: ["buffer", "buffer"], result: "i32" } as const;
const chatllm = Deno.dlopen(`${libDir}\\chatllm.dll`, {
  post_message: SYMBOLS,
  get_dialogue: SYMBOLS,
  patch_dialogue: SYMBOLS,
} as const);

type Ffi = Deno.DynamicLibrary<{
  post_message: typeof SYMBOLS;
  get_dialogue: typeof SYMBOLS;
  patch_dialogue: typeof SYMBOLS;
}>;

type Reply = {
  code?: number;
  msg?: string;
  data?: {
    uuid?: string;
    created?: boolean;
    route?: string | null;
    content?: string;
    content_html?: string;
    dialogue?: { title?: string };
    messages?: { list?: Array<{ role?: string; content?: string }> };
  };
};

function call(
  symbol: "post_message" | "get_dialogue" | "patch_dialogue",
  body: Record<string, unknown>,
): Reply {
  const input = encoder.encode(JSON.stringify(body) + "\0");
  const output = new Uint8Array(OUT_SIZE);
  const rc = (chatllm as Ffi).symbols[symbol](input, output);
  if (rc === -1) throw new Error(`${symbol}: 输出指针为空`);
  if (rc === -2) throw new Error(`${symbol}: 输出缓冲区不足（上限 ${OUT_SIZE} 字节）`);
  const end = output.indexOf(0);
  const text = decoder.decode(output.subarray(0, end === -1 ? OUT_SIZE : end));
  if (!text) throw new Error(`${symbol}: 响应为空`);
  return JSON.parse(text) as Reply;
}

function lastAssistant(reply: Reply): string {
  const messages = reply.data?.messages?.list ?? [];
  for (let i = messages.length - 1; i >= 0; i--) {
    if (messages[i].role === "assistant") return (messages[i].content ?? "").trim();
  }
  return "";
}

let uuid: string | undefined;

function prompt(prefix: string): void {
  Deno.stdout.writeSync(encoder.encode(prefix));
}

function showHistory(): void {
  if (!uuid) {
    console.log("（还没有对话）");
    return;
  }
  const reply = call("get_dialogue", { uuid, limit: 100 });
  const messages = reply.data?.messages?.list ?? [];
  if (messages.length === 0) {
    console.log("（当前对话没有消息）");
    return;
  }
  console.log(`\n--- ${reply.data?.dialogue?.title || uuid} ---`);
  for (const message of messages) {
    const who = message.role === "user" ? "你" : message.role === "assistant" ? "AI" : "系统";
    console.log(`${who} > ${(message.content ?? "").trim()}`);
  }
  console.log("---\n");
}

async function ask(text: string): Promise<void> {
  const body: Record<string, unknown> = { prompt: text };
  if (uuid) body.uuid = uuid;

  const created = call("post_message", body);
  if (created.code !== 200) {
    if (created.code === 400300 || created.code === 400301) {
      console.error(
        `对话表未初始化（code=${created.code}）。先完成初始化，或运行 ts/clear.ts --yes 重置。`,
      );
    } else {
      console.error(`发送失败: code=${created.code} ${created.msg ?? ""}`);
    }
    return;
  }

  if (created.data?.route) {
    const message = created.data.content ?? "请先完成 AI 供应商设置";
    console.log(`AI > ${message}`);
    return;
  }

  if (!uuid && created.data?.uuid) {
    uuid = created.data.uuid;
    console.log(`（新对话 ${uuid}）`);
  }

  const detail = call("get_dialogue", { uuid: uuid!, limit: 20 });
  const reply = lastAssistant(detail);
  if (!reply) {
    console.log("AI > （本轮没有文本回复）");
    return;
  }
  console.log(`AI > ${reply}`);
  if (reply.includes("no such table: setting")) {
    console.log("     ↑ 请先初始化 info.db 的 setting 表（setting_ui/setup.ts 或 info_lib）");
  }
}

// 预热：先触发库加载，避免 "[db] 加载 …" / "[designdb] 加载 …" 插在输入提示中间
try {
  call("patch_dialogue", { page: 1, page_size: 1 });
} catch {
  // 表未初始化时忽略，真正提问时会给出提示
}
try {
  call("get_dialogue", { uuid: "00000000-0000-0000-0000-000000000000", limit: 1 });
} catch {
  // 同上
}

console.log("对话已就绪：直接输入文字提问；/new 新对话，/history 查看历史，/exit 退出。\n");
console.log("（终端中 [think] / [plugins] / [reply] / [reply_base] / [db] / [designdb] / [markparse] 行是阶段日志）\n");

prompt("你 > ");
let buffer = "";
for await (const chunk of Deno.stdin.readable) {
  buffer += decoder.decode(chunk, { stream: true });
  let index = buffer.indexOf("\n");
  while (index >= 0) {
    const line = buffer.slice(0, index).replace(/\r$/, "").trim();
    buffer = buffer.slice(index + 1);
    index = buffer.indexOf("\n");

    if (line === "/exit" || line === "/quit") {
      chatllm.close();
      console.log("再见。");
      Deno.exit(0);
    }
    if (line === "/new") {
      uuid = undefined;
      console.log("（已切换到新对话，下一句会创建新会话）");
      prompt("你 > ");
      continue;
    }
    if (line === "/history") {
      showHistory();
      prompt("你 > ");
      continue;
    }
    if (!line) {
      prompt("你 > ");
      continue;
    }

    await ask(line);
    prompt("你 > ");
  }
}

chatllm.close();
