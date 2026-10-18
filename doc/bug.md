# GTLang Bug 清单 / Bug Tracker

> 记录已修复与已知 bug。按时间倒序（最新在上）。

## 已修复 / Fixed

### 优化器 / 语义

| # | 现象 | 根因 | 修复 commit |
|---|---|---|---|
| 1 | `if true { x := 2; put(x) }`（外层 `x:=1`）在 `-O2` 输出 `1`（用外层值） | 常量传播不尊重 shadowing | `eca0da5` |
| 2 | 嵌套块 `{ a:=2 { a:=3 } }` 多级 shadow 错（第二个 `put(a)` 读 `a:=3`） | codegen 裸块不隔离 + `const_if_prune` 把 `if` 变裸块丢 scope | `a5b37c5` |
| 3 | `x:=x+1; x:=x+1` 链在 `-O4` 输出错（常量传播用旧值） | `propagate_block` 先全收集再统一替换，非按序 | `a5b37c5` |
| 4 | `for i in 0..3 { put(i) }`（外层 `i:=99`）在 `-O4` 全 `99` | `propagate_block` 未移除循环变量 `var` | `a5b37c5` |
| 5 | `while i<2 { x:=i; put(x); i=i+1 }` 在 `-O4` 死循环 | `licm` 提升依赖循环内赋值变量的初值 | `a5b37c5` |
| 6 | `while i<2 { x:=i; put(x) }` 的 `x` 被常量传播成外层值 | 裸块/循环内 let 未按 codegen 语义处理 | `a5b37c5` |
| 7 | 优化器 `const_if_prune` 把 `if true { let x=... }` 变裸块，丢作用域 | 剪枝未检查分支声明 | `a5b37c5` |

### 代码生成 / 运行时

| # | 现象 | 根因 | 修复 commit |
|---|---|---|---|
| 8 | 定长数组切片 `a[1..4]` 返回垃圾值 + 越界 | `Slice` 只处理 list/str，`Ty::Array` 走 str 分支 | `dc9dedd` |
| 9 | `strcpy`/`strcat` 路径拼接缓冲区溢出（GTO/shim DLL 查找） | 无界 C 字符串操作 | `5565391` |
| 10 | `stdlib_shim.c` 缺 `<stdio.h>` 致 `snprintf` 未声明编译失败 | 生成的头文件缺 include | `5565391` |
| 11 | `gt_map_insert` 重复哈希（`find` + 插入各算一次） | 性能 bug | `dc5cd41` |
| 12 | GTO 无界通道 `realloc` 每次 +1 / `memmove` 每次 O(n) → O(n²) | 增长策略错误 | `5bf6580` |
| 13 | GTO `gto_yield` 空操作（不真让出） | 未实现 | `b284d53` |
| 14 | GTO 协程跨 worker 迁移竞态 | 协程未绑定 worker | `b284d53` |
| 15 | GTO `chan_recv_timeout`/`select` 忙等 | 无等待机制 | `4a6ef90` |

### unparse / 前端

| # | 现象 | 根因 | 修复 commit |
|---|---|---|---|
| 16 | `ClosureNew`（无捕获）被打印成 `fn_name(captures)`（错误调用） | unparse 未区分有无捕获 | `6329a51` |
| 17 | `opt/mod.rs` 注释"两段"与实际四段不符 | 文档过时 | `5565391` |

### 测试 / 基础设施

| # | 现象 | 根因 | 修复 commit |
|---|---|---|---|
| 18 | `collect_used_stmt` 漏 `DoWhile` → 误删 do-while 变量 | 漏 AST 变体 | `eb47b11` |
| 19 | CSE 对 `let mut` 误复用（`total:=0` 被替换成 `sum`） | 未区分可变 | `eb47b11` |
| 20 | 测试 `tmp_dir` 并行冲突 | 无唯一后缀 | `eb47b11` |

## 已知限制 / Known Limitations（非 bug）

| 限制 | 说明 |
|---|---|
| `len(str)` 返回字节数 | 字符数用 `utf8_len`（`import string`） |
| 高阶函数参数不能写闭包类型 `f: |T| -> T` | 省略类型，推断 |
| `json_dumps`/`json_loads` 仅字符串转义 | 无对象序列化，只接受 `str` |
| `while` 无 `else` | 仅 `for ... else` |
| 泛型结构体字面量不写 `[T]` | `Box { v: 42 }`（推断） |
| `set` 用 `insert`（非 `push`） | `push` 只接受 list |
| `startswith`/`endswith`（非 `starts_with`） | `import string` |
| `strip(s, chars)` 需 2 参 | 去空白传 `""` |
| `find(s, sub)` 未找到返回 -1 | `import string` |
| 元组解构仅 `let (a,b) = ...` | 不支持 `a,b := (..)` |
| list comp 的 `in` 不支持范围 | `[x for x in list]` |
| 元组不能作 match 模式 | `(1,2) =>` 明确拒绝 |
| `format` 的 args 用 `\n` 分隔 | 非 list |
| `impl[T]` 泛型 impl 不支持 | 泛型方法需其他方式 |
| 裸 `{}` 块不引入作用域 | 解构 `let (a,b)` 依赖此 |
| `serve_fn` 已随解释器移除 | 原 `login_app`/`todo_app` 不可编译 |
| `loop_lcg` 的 C 块无法内联 | C 块跨编译单元 + 全局状态（LTO 已尽力） |
| `defer` 函数级、编译期登记 | 循环内只登记 1 次（非 Go 运行时累积）；局部变量引用在函数末尾可能失效 |
| `chan` 双层函数指针转发 | `gto.dll` 运行期加载，架构必需 |
| GC 环回收默认关闭 | `GC_COLLECT_ON=0`，需正确插桩才安全 |
