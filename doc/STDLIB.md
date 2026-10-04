# GTLang 标准库参考 / Standard Library Reference

> 覆盖 `sql` / `crypto` / `entropy` / `session` / `web` / `http` / `net` 等模块。
> **使用前必须 `import <模块>`**（如 `import sql`）。

## 内存归属（重要）

- [sql — SQLite](#sql)
- [crypto — 哈希与密码](#crypto)
- [entropy — 真随机](#entropy)
- [session — 会话](#session)
- [web — 网页服务器](#web)
- [http — HTTP 客户端](#http)
- [net — TCP](#net)

---

<a id="sql"></a>
## sql — SQLite

```gt
import sql

db := sql_open("app.db")        // 打开/创建；":memory:" 为内存库
sql_exec(db, "CREATE TABLE t (id INTEGER, name TEXT)")

// 单条执行（返回 0 成功）
sql_exec(db, "INSERT INTO t VALUES (1, 'a')")

// 批量（单事务，快 ~12x；失败自动回滚，返回出错行号或 0）
sql_exec_many(db, "INSERT INTO t VALUES (2, 'b')\nINSERT INTO t VALUES (3, 'c')")

// 手动事务
sql_begin(db)
sql_exec(db, "INSERT INTO t VALUES (4, 'd')")
sql_commit(db)   // 或 sql_rollback(db)

// 查询：返回 "列1\t列2\n行...\n"（首行是列名）
rows := sql_query(db, "SELECT id, name FROM t")
put(rows)

sql_close(db)
```

| 函数 | 说明 |
|---|---|
| `sql_open(path)` | 打开数据库（`:memory:` 内存库），返回句柄（0=失败）|
| `sql_close(db)` | 关闭 |
| `sql_exec(db, sql)` | 执行一条 SQL，返回 0 成功 |
| `sql_exec_many(db, text)` | 单事务执行多行（`\n` 分隔），出错回滚 |
| `sql_begin/commit/rollback(db)` | 事务控制 |
| `sql_query(db, sql)` | 查询，返回制表符分隔的文本 |
| `sql_run(db, sql)` | 执行，成功返回 1 |
| `sql_error(db)` | 最近错误信息 |

**注意**：底层用 Windows 自带的 `winsqlite3.dll`（Win10+）。

---

<a id="crypto"></a>
## crypto — 哈希与密码

```gt
import crypto

sha256("abc")            // 64 位十六进制
sha512("abc")            // 128 位
sha1("abc")              // 40 位
md5("abc")               // 32 位
hmac_sha256("key", "msg")

hex_encode("AB")         // -> "4142"
hex_decode("4142")       // -> "AB"

// 密码哈希（salt$sha256(salt+pwd)）
h := password_hash("pw", "salt")
password_verify("pw", h)    // true
password_verify("bad", h)   // false
```

| 函数 | 说明 |
|---|---|
| `sha256/sha512/sha1/md5(s)` | 十六进制摘要 |
| `hmac_sha256(key, msg)` | HMAC-SHA256 |
| `hex_encode/hex_decode(s)` | 十六进制编解码 |
| `password_hash(pwd, salt)` | 密码哈希 |
| `password_verify(pwd, stored)` | 密码校验（bool）|
| `random_hex(n)` | 十六进制随机（**伪随机**，安全性请用 `entropy`）|

> ⚠️ `password_hash` 为**单次 SHA-256**（演示用）；生产环境请自行迭代加盐。
> 💡 `random` 模块的 RNG 状态是**线程局部**的，且初值按"时间 + 地址"扰动，
> 因此并发 `go` 的多个线程不会拿到相同序列（需确定性复现时请显式 `seed(n)`）。

---

<a id="entropy"></a>
## entropy — 真随机（CSPRNG）

```gt
import entropy

entropy_random_hex(16)   // 32 个十六进制字符（真随机）
entropy_uuid()           // RFC 4122 v4 UUID
entropy_random_int(100)  // [0, 100) 均匀随机
entropy_random_bytes(8)  // 8 个原始字节
```

底层 Windows `BCryptGenRandom`（系统 CSPRNG）。

---

<a id="session"></a>
## session — 服务端会话

```gt
import sql
import session

db := sql_open("app.db")
sid := session_create(db, "alice", 3600)   // 生成会话 id（48 字符），存活 3600 秒
user := session_get(db, sid)               // 取用户名（过期/不存在返回 ""）
session_count(db)                          // 有效会话数
session_destroy(db, sid)                   // 删除
session_gc(db)                             // 清理过期
sql_close(db)
```

会话 id 由 `entropy`（CSPRNG）生成，存 SQLite 表 `sessions`。

---

<a id="web"></a>
## web — 网页服务器

### 模式一：路由表（`serve`）

```gt
import web

routes := "GET / =" + html_page("首页", "<h1>你好</h1>") + "\n" +
          "GET /hello/:name =你好 {{param:name}}\n" +
          "GET /static/* =@dir:static\n" +
          "POST /echo =收到：{{body}}\n" +
          "ANY / =未找到"
serve(8080, routes)
```

**路由行格式**：`METHOD /path = 响应`。响应可以是：
- **纯文本**（默认 `text/html`）—— 支持占位符
- `{status}` 前缀指定状态码（如 `{404}`）
- `{Header: value}` 前缀添加响应头（如 `{Set-Cookie: a=1}`）
- `@file:路径` / `@dir:目录` 返回文件（按扩展名猜 MIME）

**占位符**：
| 占位 | 含义 |
|---|---|
| `{{body}}` | POST 请求体 |
| `{{param:name}}` | 路由参数（`:name`）|
| `{{query:name}}` | 查询参数（`?name=x`）|
| `{{header:Name}}` | 请求头 |
| `{{cookie:Name}}` | Cookie 值 |
| `{{form:Name}}` | 表单字段（`application/x-www-form-urlencoded` 请求体）|

**通配符**：`:name` 匹配单段；`/*name` 贪婪匹配剩余整条路径（注入 `{{param:name}}`）。

### 模式二：处理函数回调（`serve_fn`，**仅 --run / JIT**）

```gt
import web

fn 处理(req: str) -> str {
    // req = "METHOD /path\n请求体\n---HEADERS---\nHeader: v\n..."
    return html_page("标题", "<h1>动态响应</h1>")
}

fn main() { serve_fn(8080, 处理) }
```

处理函数返回响应串（同样支持 `{status}{Header}` 前缀）。

> ⚠️ `serve_fn` 依赖函数地址，**仅解释器（`--run`）支持**；编译为 exe（`--c`）请用 `serve`。

### 辅助函数

| 函数 | 说明 |
|---|---|
| `html_page(title, body)` | 生成 HTML 页面 |
| `html_escape(s)` | HTML 转义 |
| `url_encode/url_decode(s)` | URL 编解码 |
| `parse_query(q)` / `query_get(qs, k)` | 查询串解析 |
| `route_match(pat, path)` | 路由匹配（返回参数 `k=v;k=v`）|

---

<a id="http"></a>
## http — HTTP 客户端

```gt
import http

body := http_get("https://example.com")
http_post("https://api.example.com", "{\"k\":1}")
http_put(...) / http_delete(...)
http_request("GET", url, "")
http_download(url, "file.bin")
http_status(url)   // HTTP 状态码
```

HTTP 用 `TcpStream`；HTTPS 用 Windows `WinHttp`（Win7+）。

---

<a id="net"></a>
## net — TCP

```gt
import net

conn := tcp_connect("example.com", 80)
net_send(conn, "GET / HTTP/1.0\r\n\r\n")
data := recv_all(conn)
net_close(conn)

l := tcp_listen(8080)
c := accept(l)
close_listener(l)
```

---

## 完整示例

- `examples/http_server.gt` — 路由表 + 静态文件 + JSON
- `examples/todo_app.gt` — 留言板（web + sqlite）
- `examples/login_app.gt` — 注册/登录/后台（web + sql + crypto + session + entropy）