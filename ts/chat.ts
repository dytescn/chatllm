const res = await fetch("http://127.0.0.1:44944/libchatllm", {
  method: "POST",
  headers: {
    "Content-Type": "application/chat",
    "FFI-Symbol": "chatllm",
  },
  body: JSON.stringify({
     prompt: "你好，你是谁？",
     max_tokens: 128
  }),
});

console.log(await res.json());
