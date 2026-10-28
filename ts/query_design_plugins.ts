const res: any = await fetch("http://127.0.0.1:44944/database", {
  method: "POST",
  headers: {
    "Content-Type": "application/libary",
    "FFI-Symbol": "db_query",
  },
  body: JSON.stringify({
    path: "design_plugins",
    sql: "SELECT COUNT(*) AS n FROM design_plugins",
  }),
});

console.log(await res.json());
