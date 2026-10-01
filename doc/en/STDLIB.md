# GTLang Standard Library Reference

> Covers `sql` / `crypto` / `entropy` / `session` / `web` / `http` / `net`.
> **You must `import <module>` first** (e.g. `import sql`).

## sql — SQLite

```gt
import sql
db := sql_open("app.db")            // ":memory:" for in-memory
sql_exec(db, "CREATE TABLE t (id INTEGER, name TEXT)")
sql_exec(db, "INSERT INTO t VALUES (1, 'a')")
sql_exec_many(db, "INSERT INTO t VALUES (2, 'b')\nINSERT INTO t VALUES (3, 'c')")
sql_begin(db) ... sql_commit(db)    // or sql_rollback(db)
rows := sql_query(db, "SELECT id, name FROM t")   // tab-separated, first line = columns
sql_close(db)
```

Backed by Windows `winsqlite3.dll`. `sql_exec_many` runs all lines in one transaction (~12x faster) and rolls back on error.

## crypto — hashes & passwords

```gt
import crypto
sha256("abc")   sha512("abc")   sha1("abc")   md5("abc")
hmac_sha256("key", "msg")
hex_encode("AB")   hex_decode("4142")
h := password_hash("pw", "salt")
password_verify("pw", h)   // true
```

> `password_hash` is single-pass SHA-256 (demo). Use iteration/salt for production.

## entropy — CSPRNG

```gt
import entropy
entropy_random_hex(16)   // 32 hex chars
entropy_uuid()           // RFC 4122 v4
entropy_random_int(100)  // [0, 100)
entropy_random_bytes(8)
```

Backed by Windows `BCryptGenRandom`.

## session — server-side sessions

```gt
import sql
import session
db := sql_open("app.db")
sid := session_create(db, "alice", 3600)   // 48-char id, 1h TTL
session_get(db, sid)        // username, "" if missing/expired
session_count(db)
session_destroy(db, sid)
session_gc(db)
```

## web — web server

### Route table (`serve`)

```gt
import web
routes := "GET / =" + html_page("Hi", "<h1>Hello</h1>") + "\n" +
          "GET /hello/:name =Hi {{param:name}}\n" +
          "GET /static/* =@dir:static\n" +
          "POST /echo =got: {{body}}"
serve(8080, routes)
```

Response may carry `{status}` and `{Header: value}` prefixes, or `@file:` / `@dir:`.
Placeholders: `{{body}}` `{{param:name}}` `{{query:name}}` `{{header:Name}}` `{{cookie:Name}}`.

### Handler callback (`serve_fn`, **--run / JIT only**)

```gt
import web
fn handle(req: str) -> str {
    // req = "METHOD /path\nbody\n---HEADERS---\nHeader: v\n..."
    return html_page("Title", "<h1>dynamic</h1>")
}
fn main() { serve_fn(8080, handle) }
```

> `serve_fn` needs a function address, so it is **JIT-only**; use `serve` when compiling to an exe.

## http — HTTP client

```gt
import http
http_get(url)   http_post(url, body)   http_put(url, body)   http_delete(url)
http_request(method, url, body)   http_download(url, path)   http_status(url)
```

HTTP via TcpStream; HTTPS via Windows WinHttp (Win7+).

## net — TCP

```gt
import net
conn := tcp_connect(host, port)
net_send(conn, data)   recv_all(conn)   net_close(conn)
l := tcp_listen(port)   c := accept(l)   close_listener(l)
```

## Examples

- `examples/http_server.gt` — routes + static + JSON
- `examples/todo_app.gt` — guestbook (web + sqlite)
- `examples/login_app.gt` — register/login/admin (web + sql + crypto + session + entropy)