/**
 * chatllm 测试的共享 Deno FFI 层。
 *
 * 直接加载 testenv/libary 下的 chatllm / database / kv_lib 动态库，
 * 不再经过 cyctron 的 HTTP 网关。测试脚本把工作目录切到 testenv，
 * 这样各库内部的 `./libary/...`、`./database/...` 相对路径与生产一致。
 */

export type Json = Record<string, any>;

const encoder = new TextEncoder();
const decoder = new TextDecoder();

function toLocalPath(url: URL): string {
  let path = decodeURIComponent(url.pathname);
  if (Deno.build.os === "windows") {
    if (/^\/[A-Za-z]:/.test(path)) path = path.slice(1);
    path = path.replaceAll("/", "\\");
  }
  return path;
}

const testenvDir = toLocalPath(new URL("../testenv/", import.meta.url));
const libDir = `${testenvDir}${Deno.build.os === "windows" ? "libary\\" : "libary/"}`;

const currentPath = Deno.env.get("PATH") ?? "";
const separator = Deno.build.os === "windows" ? ";" : ":";
if (!currentPath.split(separator).includes(libDir.replace(/[\\/]$/, ""))) {
  Deno.env.set("PATH", `${libDir}${separator}${currentPath}`);
}
Deno.chdir(testenvDir);

const SYMBOLS = { parameters: ["buffer", "buffer"], result: "i32" } as const;

function libFile(base: string, windows: string, macos: string, linux: string): string {
  const file = Deno.build.os === "windows"
    ? windows
    : Deno.build.os === "darwin"
    ? macos
    : linux;
  return `${libDir}${file}`;
}

const chatllm = Deno.dlopen(libFile("chatllm", "chatllm.dll", "chatllm.dylib", "chatllm.so"), {
  post_chat: SYMBOLS,
  post_call_chat: SYMBOLS,
  post_chat_detail: SYMBOLS,
  post_dialogue_list: SYMBOLS,
  post_dialogue_search: SYMBOLS,
  post_dialogue_update: SYMBOLS,
  post_message: SYMBOLS,
  get_dialogue: SYMBOLS,
  patch_dialogue: SYMBOLS,
  update_dialogue: SYMBOLS,
} as const);

const database = Deno.dlopen(
  libFile("database", "database.dll", "libdatabase.dylib", "libdatabase.so"),
  {
    db_create: SYMBOLS,
    db_insert: SYMBOLS,
    db_update: SYMBOLS,
    db_delete: SYMBOLS,
    db_query: SYMBOLS,
  } as const,
);

const kvlib = Deno.dlopen(libFile("kv_lib", "kv_lib.dll", "kv_lib.dylib", "kv_lib.so"), {
  kv_collection: SYMBOLS,
  kv_embed: SYMBOLS,
  kv_search: SYMBOLS,
} as const);

type Lib = Deno.DynamicLibrary<Record<string, typeof SYMBOLS>>;

function callLib(lib: Lib, symbol: string, body: Json, bufferSize: number): Json {
  const symbols = lib.symbols as Record<string, (i: Uint8Array, o: Uint8Array) => number>;
  const fn = symbols[symbol];
  if (!fn) throw new Error(`符号不存在：${symbol}`);

  const encoded = encoder.encode(JSON.stringify(body));
  const input = new Uint8Array(encoded.length + 1);
  input.set(encoded);
  input[encoded.length] = 0;

  const output = new Uint8Array(bufferSize);
  const rc = fn(input, output);
  if (rc === -1) throw new Error(`${symbol}: json_out 为空指针`);
  if (rc === -2) throw new Error(`${symbol}: 响应超过 ${bufferSize} 字节`);

  const end = output.indexOf(0);
  const text = decoder.decode(output.subarray(0, end === -1 ? output.length : end));
  if (!text) throw new Error(`${symbol}: 库未写入输出（rc=${rc}）`);
  return JSON.parse(text) as Json;
}

/** 基础符号直接用；其余按 cyctron 约定补 `post_` 前缀。 */
const CHATLLM_BASE = new Set(["get_dialogue", "patch_dialogue", "update_dialogue"]);

export function callChatllm(symbol: string, body: Json = {}): Json {
  const name = symbol.startsWith("post_") || CHATLLM_BASE.has(symbol)
    ? symbol
    : `post_${symbol}`;
  return callLib(chatllm as Lib, name, body, 64 * 1024);
}

function normalizeDbPath(path: unknown): unknown {
  if (typeof path !== "string") return path;
  return path.replace(/^\.\//, "");
}

export function callDatabase(symbol: string, body: Json = {}): Json {
  const payload = { ...body, path: normalizeDbPath(body.path) };
  return callLib(database as Lib, symbol, payload, 1024 * 1024);
}

/**
 * kv_lib 0.1.5 只保留 collection / embed / search 三个符号。
 * 旧测试使用的 kv_record / kv_collection 写入按新接口转换：
 * - put → kv_embed（按 id 文本组写入并向量化）
 * - get → kv_search（按 key 文本检索；找不到时返回空 data）
 * - collection put → 空操作（集合由 kv_embed 自动创建）
 */
export function callKv(symbol: string, body: Json = {}): Json {
  if (symbol === "kv_collection") {
    if (body.action === "put" || body.action === "delete") {
      return { code: 200, data: {}, msg: "success" };
    }
    return callLib(kvlib as Lib, "kv_collection", body, 64 * 1024);
  }

  if (symbol === "kv_search") {
    return callLib(kvlib as Lib, "kv_search", body, 1024 * 1024);
  }

  if (symbol === "kv_record") {
    if (body.action === "put") {
      const item = body.items?.[0] ?? {};
      const response = callLib(kvlib as Lib, "kv_embed", {
        collection: body.collection,
        id: item.key,
        texts: [item.text ?? item.value ?? ""],
      }, 64 * 1024);
      return { code: response.code, data: response.data, msg: response.msg };
    }
    if (body.action === "get") {
      const key = body.keys?.[0];
      if (!key) return { code: 200, data: [] };
      const response = callLib(kvlib as Lib, "kv_search", {
        collection: body.collection,
        query: key,
        top_k: 1,
        with_value: true,
      }, 1024 * 1024);
      const list = (response.data?.list ?? []) as Json[];
      return { code: response.code, data: list, msg: response.msg };
    }
    if (body.action === "delete") return { code: 200, data: {}, msg: "success" };
  }

  throw new Error(`不支持的 kv_lib 调用：${symbol} ${body.action ?? ""}`);
}

/** 兼容旧测试的 `post(api, symbol, body)` 形式（同步返回）。 */
export function post(api: string, symbol: string, body: Json = {}): Json {
  switch (api) {
    case "chatllm":
      return callChatllm(symbol, body);
    case "database":
      return callDatabase(symbol, body);
    case "kv_lib":
      return callKv(symbol, body);
    default:
      throw new Error(`未知能力库：${api}`);
  }
}

export function closeLibs(): void {
  chatllm.close();
  database.close();
  kvlib.close();
}
