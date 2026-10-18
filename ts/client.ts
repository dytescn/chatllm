// ts/client.ts
export const BASE = "http://127.0.0.1:44944/chatllm";

export async function call(symbol: string, body: unknown) {
  const res = await fetch(BASE, {
    method: "POST",
    headers: {
      "Content-Type": "application/libary",
      "FFI-Symbol": symbol,
    },
    body: JSON.stringify(body),
  });
  return await res.json();
}

export function section(title: string) {
  console.log(`\n=== ${title} ===`);
}

export function assert(cond: boolean, msg: string) {
  if (cond) {
    console.log(`✅ ${msg}`);
  } else {
    console.error(`❌ ${msg}`);
    process.exitCode = 1;
  }
}
