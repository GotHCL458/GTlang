# GTLang 编译器缺陷修复记录（Bug Log）

> 本文件记录本次深入测试/模糊测试（fuzz）会话中发现并修复的全部真实缺陷。
> 每条含：**现象 → 根因 → 修复 → 验证**。按发现顺序编号。
> 基线：会话开始前测试为 **626**（25 单元 + 101 双后端一致性 + 500 前端批量）。

## 概览

| # | 类别 | 缺陷 | 发现方式 |
|---|---|---|---|
| 1 | 双端一致性 | Windows 下 JIT 与 AOT 的换行字节不一致（CRLF vs LF）| 审计 |
| 2 | 闭包/一等函数 | 函数返回闭包后不可调用 | 测试 |
| 3 | 闭包/高阶 | 高阶闭包参数推断（`g(f(x))`）失败 | 测试 |
| 4 | AST 遍历 | `each_expr` 遍历覆盖不全 | 审计 |
| 5 | 闭包 | 闭包在 `if`/`match`/`slice` 等子表达式里调用不被识别 | 测试 |
| 6 | 闭包捕获 | 闭包捕获容器/字符串失败 | 测试 |
| 7 | 闭包捕获 | 闭包捕获外层参数（`return \|x\| g(f(x))`）失败 | 测试 |
| 8 | AST 遍历 | （与 4/5 同源）遍历缺口 | 审计 |
| 9 | JIT | `gen_call_value` 误用 `e.ty`（应为 callee 的闭包签名）| 测试 |
| 10 | mono | 未替换推断出的 `ret_ty`（泛型多实例类型混用）| 测试 |
| 11 | sema | 纯赋值因 RHS 为 `Unknown` 而改变量类型 | 测试 |
| 12 | sema | `Closure` 参数返回类型误推为 `i64` | 测试 |
| 13 | sema | `Result(_, str)` 多 `return` 的 `ret_ty` 合并错误（双端崩溃）| 测试 |
| 14 | 诊断 | `ariadne` 的 `Span` 未对齐 UTF-8 字符边界 → 截断多字节源码 panic | fuzz |
| 15 | parser | `block()` 重置 `depth` → 深嵌套 `if`/`while`/`for` 爆栈 | 边界 |
| 16 | parser | `\|>` 右侧非可调用时静默丢弃 LHS（死循环/静默错值）| fuzz |
| 17 | parser | `match` 守卫被三元分支误吞（`pat if cond =>` 不可用）| 文档核对 |
| 18 | parser | `"${x}"`（整串单个插值）退化为内部表达式 `x` | 文档核对 |
| 19 | parser | `if` 语句与前一语句同行时被误判为三元 → `if/elif/else` 全不可用 | fuzz |
| 20 | sema | 非枚举 `match` 的穷尽性检查逻辑反了：空 `match` 反而通过 | fuzz |
| 21 | sema/JIT/LLVM | `match v { n if n > 0 }` 裸标识符绑定未实现 | fuzz |
| 22 | sema/JIT/LLVM | 嵌套解构 `Some(Some(v))`/`Ok(Some(v))` 未支持 | fuzz |
| 23 | codegen | 复合类型（元组）作 `match` 模式生成非法 IR | fuzz |
| 24 | sema/JIT/LLVM | enum 载荷内的解构 `E::A(Some(v))` 未支持 | fuzz |
| 25 | JIT/LLVM | enum 载荷解构缺少内层 tag 判定（`E::A(Some)` vs `E::A(None)`）| fuzz |
| 26 | own | match 绑定名与外层同名时误报 use-after-move | fuzz |
| 27 | sema/mono/type | 借用上的方法调用 `a.方法()`（`a: &T`）不可用 | fuzz |
| 28 | mono | 借用类型函数参数上的方法调用 `p.方法()`（`p: &P`）不可用 | fuzz |
| 29 | 文档 | `gtfmt` 格式化器已移除但文档仍承诺（遗留二进制会破坏插值）| 文档核对 |
| 30 | sema/mono | `dyn` 形参 + `struct` 实参（缺 `Struct→Dyn` 兼容与自动装箱）| fuzz |
| 31 | opt | 内联时 `recv.方法()` 的接收者未被替换 | fuzz |
| 32 | codegen | `dyn` 方法返回 `str`（ptr 类）按 `i64` 生成非法 IR | fuzz |
| 33 | codegen | `dyn` 方法返回 `bool` 按 `i64` 而非 `i1` 生成非法 IR | fuzz |
| 34 | opt/mono | `collect_calls`/`auto_box_args` 缺 `MethodOn` 等分支 → 死代码误删 | fuzz |
| 35 | module | 跨模块 `prefix_struct_ty` 漏 `Dyn`/`Result`/`Option`/`Tuple`/`Ref`/`Closure`/`Enum` | fuzz |
| 36 | parser | **`Option[T]` 方括号写法被解析成 `T`** | fuzz |
| 37 | parser | **泛型 struct 参数化 `盒[T]` 被解析成 `T`** | fuzz |
| 38 | sema/codegen/jit | **`x or y`（`x` 为 `Result`）出错**（`Some(v)` 对 `Result` 绑定了 Err 载荷）| fuzz |
| 39 | sema/JIT | **无值 `return` + 非 void 返回类型**：`--check` 通过但 JIT panic（不完整 IR）| fuzz |
| 40 | hoist | **`go 工(ch, f(i))`（go 实参含闭包调用）** → codegen/JIT `undefined function` | fuzz |
| 41 | hoist | **`go 工(加一, 5)`（go 实参传函数名）** → `undefined variable` | fuzz |
| 42 | JIT/codegen | **enum 解构的守卫 `E::A(n) if n > 5`** → guard 在载荷绑定前求值，`undefined variable 'n'` | fuzz |
| 43 | codegen | **`match true { true => ... }`（bool 主体）** → AOT 非法 IR（`icmp eq i64 true, true`）| fuzz |
| 44 | sema | **`set()` 元素类型未从 `insert` 细化** → `for k in set` 输出原始句柄值 | fuzz |
| 45 | sema/JIT/codegen | **`map` 的 `f64` 值**（`m["k"] = 3.5` / `m["k"]`）：JIT 垃圾值、AOT 非法 IR | fuzz |
| 46 | JIT/codegen | **`list`/`map` 的 `f64` 元素**（`push(l, 3.5)` / `l[0]`）：JIT 垃圾值、AOT 非法 IR（第 45 的根因统一修复）| fuzz |
| 47 | sema | **list 非整数下标 / for 非整数边界**（崩溃 / 死循环）| fuzz |
| 48 | parser | **元组类型 `-> (T, T)` 解析失败** | fuzz |
| 49 | mono | **泛型函数前向引用失败** | fuzz |
| 50 | mono | **`subst_ty_a` 缺 Tuple/Option/Ref** | fuzz |
| 51 | mono | **`where T: Ord` 对 int 报错** | fuzz |
| 52 | hoist | **泛型函数名与变量同名 → 误转闭包** | fuzz |
| 53 | sema/mono/codegen | **`list[dyn Trait]` 容器**（未装箱/裸指针/ABI 不匹配）| fuzz |

---

## 详细说明

### 1. Windows 下 JIT/AOT 换行字节不一致
- **现象**：同一程序，JIT 与 AOT 的 stdout 在 Windows 上不完全一致（CRLF vs LF）。
- **根因**：CRT 的 stdout 文本模式；一致性测试此前做了 `\r\n`→`\n` 归一化，掩盖了该问题。
- **修复**：两个运行时的 stdout 都设为二进制模式；一致性测试**取消归一化**，让该类缺陷暴露。

### 2-8. 闭包与一等函数系列
- **现象**：函数返回闭包不可调用；高阶闭包参数推断失败；闭包在子表达式里调用不被识别；捕获容器/字符串/外层参数失败。
- **根因**：闭包提升（`hoist`）与调用改写（`convert_closure_calls`）覆盖不全；AST 遍历器遗漏变体；缺少外层变量可见性跟踪。
- **修复**：补全 `each_expr`/`convert_closure_calls_expr` 的变体；用 `thread_local` 跟踪外层可见变量；闭包工厂识别 `return |x| ...`。

### 13. `Result(_, str)` 多 return 的 ret_ty 合并
- **现象**：`Ok` 载荷为 `i64`、`Err` 载荷为 `str` 的 `Result` 函数，多个 `return` 合并后错推为 `Unknown`，双端崩溃。
- **根因**：`join_ret_types` 未对 `Result`/`Option`/`Tuple` 逐字段 `type_join`。
- **修复**：逐字段合并。

### 14. ariadne 的 Span 未对齐 UTF-8 边界
- **现象**：源码被截断在多字节字符中间时，诊断渲染 panic（`end byte index is not a char boundary`）。
- **根因**：`render_diags` 的 `Span` 未对齐字符边界。
- **修复**：主 `Span` 与 `notes` 的 `Span` 都向最近字符边界对齐。

### 15. `block()` 重置 depth → 深嵌套爆栈
- **现象**：500 层 `if`/`while`/`for`/`match`/`try` 爆栈。
- **根因**：`block()` 每次 `self.depth = 0`，块嵌套不累加。
- **修复**：`block()` 改为 `enter_depth/leave_depth`；每函数独立由 `fn_def` 处理。

### 16. `\|>` 静默丢弃 LHS
- **现象**：`i \|> i + 1` 变成 `i + 1`（无声错值/死循环）。
- **根因**：`\|>` 右侧非 `Call`/`Ident` 时直接 `other`（丢弃 lhs）。
- **修复**：包成 `CallValue { callee: rhs, args: [lhs] }`，由 sema 报“不可调用”。

### 17. match 守卫被三元吞
- **现象**：`match v { n if n > 0 => ... }` 报 “ternary expects 'else'”。
- **根因**：pattern 用 `expr(0)`，触发三元分支。
- **修复**：pattern 用 `expr(1)`（屏蔽三元/`or`/`\|>`）。

### 18. `"${x}"` 单插值退化
- **现象**：`"${x}"`（整串只有一个插值）被当成 `x` 本身（类型/值都错）。
- **根因**：`interp` 对“单个 `Expr`”做了“直接返回内部表达式”的优化。
- **修复**：只有“单个 `Lit`”才退化；单个 `Expr` 保留为 `Interp`。

### 19. `if` 同行被误判三元
- **现象**：`a := 1  if a > 0 { ... }` 报 “ternary expects 'else'”，`if/elif/else` 全不可用。
- **根因**：三元分支只要求 `if` 同行，未确认存在 `else`。
- **修复**：三元分支记录 `pos`，无 `else` 时回退，交给语句层。

### 20. 非枚举 match 穷尽性逻辑反了
- **现象**：空 `match`（无分支）被判定“穷尽”而通过。
- **根因**：`has_value`（有字面量分支）才报“不穷尽”，逻辑反了。
- **修复**：`I64/F64/Str` 主体**无条件**要求 `_` 兜底。

### 21-25. match 解构系列
- **21**：裸标识符绑定 `n if n > 0` 未实现（文档承诺）——sema/JIT/LLVM 三处补。
- **22**：嵌套解构 `Some(Some(v))`/`Ok(Some(v))` —— 三处改为递归深解构。
- **23**：复合类型（元组）作模式生成非法 IR —— 双端明确拒绝。
- **24**：enum 载荷内解构 `E::A(Some(v))` —— 三处 enum 绑定改为递归。
- **25**：enum 载荷解构缺内层 tag 判定 —— JIT 移到 arm 块内判定；LLVM 的 `cond` 加内层 tag `and`。

### 26. own 误报 use-after-move
- **现象**：`match e { ... E::A(Err(e)) => ... }` 中内层 `e` 与外层参数同名 → 误报。
- **根因**：`own.rs` 把 pattern 里的标识符当“对外层的使用”。
- **修复**：pattern 里的裸标识符/解构构造是“绑定”，跳过。

### 27-28. 借用的方法调用
- **27**：`a := &p; a.方法()` 不可用 —— `type.rs` 允许 `&T→T`；`sema` 查方法剥 `Ref`；`mono` 降级剥 `Ref`。
- **28**：借用参数 `fn f(p: &P) { p.方法() }` 不可用 —— `mono` 把函数参数（剥 `Ref`）加入降级变量表。

### 29. gtfmt 文档漂移
- **现象**：文档承诺 `gtfmt`，但源码已移除；遗留 `target/release/gtfmt.exe` 会把 `$a` 格式化成 `$ {a}`（静默改错语义）。
- **修复**：删除文档中的 `gtfmt` 承诺（中英 README、`doc/syntax_status.md`、`.cuckoo` 文档）。

### 30. dyn 形参 + struct 实参
- **现象**：`fn f(s: dyn Trait)` 接收 `struct` 实参被拒；且无自动装箱。
- **修复**：`is_assignable` 允许 `Struct/Enum → Dyn`；`mono` 新增实参自动装箱 pass（含 `MethodOn` 等分支）。

### 31. 内联未替换 recv.方法() 的接收者
- **现象**：`fn f(s: dyn T) { s.面积() }` 被内联进调用点后 `s` 未定义。
- **根因**：`rename_expr` 的 `Call(name)` 只替换 `args`，不替换 `name` 里的接收者。
- **修复**：内联时替换 `recv.方法` 的接收者；实参不是简单标识符则放弃内联。

### 32-33. dyn 方法返回类型
- **32**：返回 `str`（ptr 类）按 `i64` → 非法 IR。`ret_llvm` 保持 `ptr`。
- **33**：返回 `bool` 按 `i64` 而非 `i1` → 非法 IR。`ret_llvm` 保持 `i1`。

### 34. collect_calls / auto_box_args 漏分支
- **现象**：`造(...).我()` 里的 `造` 只出现在 `MethodOn.recv` → 被死代码消除误删。
- **根因**：`opt::collect_calls` 与 `mono::auto_box_args_expr` 缺 `MethodOn`/`Borrow`/`DynBox`/`Slice`/`ListComp`/`EnumLit`/`TupleLit` 分支。
- **修复**：补全；并加 `collect_calls`/`rename_expr` 的**遍历完备性单元测试**守护。

### 35. 跨模块类型前缀漏 Dyn 等
- **现象**：跨模块 `import` 后 `dyn Trait` 方法调用报 undefined。
- **根因**：`prefix_struct_ty`/`rewrite_struct_ty` 只处理 `Struct`/`List`/`Set`/`Map`/`Array`。
- **修复**：递归 `Dyn`/`Enum`/`Result`/`Option`/`Tuple`/`Ref`/`Closure`。

### 36. `Option[T]` 被解析成 `T`
- **现象**：`-> Option[int]` 被当成 `int`（`return Some(x)` 报类型不符）。
- **根因**：`parser_type` 的参数化类型 `match` 无 `Option` 分支，落入 `_ => first`。
- **修复**：加 `"Option"` 分支。

### 37. 泛型 struct 参数化 `盒[T]` 被解析成 `T`
- **现象**：`fn 造[T](x: T) -> 盒[T]` 返回类型被当成 `T`。
- **根因**：同上，`_ => first`。
- **修复**：`_` 分支保留外层名字（`Ty::Struct(name)`），参数由 mono 推断。

### 38. `x or y` 对 Result 出错
- **现象**：`f(2) or -1`（`f` 返回 `Result`）AOT 生成非法 IR。
- **根因**：`or` 展开为 `match x { Some(v)=>v, Ok(v)=>v, _=>y }`；`Some(v)` 对 `Result` 主体时，codegen/JIT 按 `Result` 的第二载荷（Err 类型）绑定。
- **修复**：`Some(v)` 对 `Result`、`Ok(v)` 对 `Option` 视为不匹配（绑定类型 `Unknown`，tag 判定自然不匹配）。


### 39. 无值 `return` + 非 void 返回类型
- **现象**：`fn f() -> int { return }` / `match` arm 里无值 `return` —— `--check` 通过，但 `--run`（JIT）panic（Cranelift `lower.rs` 的 `Option::unwrap()` on `None`，不完整 IR）。
- **根因**：`sema` 只在 `infer_block_ret` 里处理 `Return(Some(e))`，漏了 `Return(None)`；显式标注返回类型的函数又跳过了块遍历。
- **修复**：`infer_block_ret` 处理 `Stmt::Return(None)`（非 void 时返回 E301）；显式标注的函数也走一遍块遍历；`sema.rs` 收集该 `Err` 到 errors。

### 40. go 实参含闭包调用
- **现象**：`go 工(ch, f(i))`（`f` 是闭包变量）—— `--check` 通过，但 codegen/JIT 报 `undefined function 'f'`。
- **根因**：`hoist::convert_closure_calls` 的 `Stmt` 匹配缺 `Stmt::Go`（同样缺 `Throw`/`Try`/`Labeled`/`LocalFn`）。
- **修复**：补全这些语句分支（Go 的 args、Throw 的值、Try 的 body/catches/fin、Labeled 内层、LocalFn 的 body）。

### 41. go 实参传函数名
- **现象**：`go 工(加一, 5)`（`加一` 是顶层函数名，作一等值）—— `undefined variable '加一'`。
- **根因**：`hoist::convert_fn_refs_block` 与 `collect_free_vars_block` 缺 `Stmt::Go` 等分支，函数名未转成闭包值。
- **修复**：这两个遍历器同样补全 `Go`/`Throw`/`Try`/`Labeled`/`LocalFn`。
- **守护**：新增“语句遍历完备性”单元测试（`convert_closure_calls`）。

### 42. enum 解构的守卫在绑定前求值
- **现象**：`E::A(n) if n > 5 => ...` —— `--check` 通过，`--run` 报 `undefined variable 'n'`（JIT 与 LLVM 都错）。
- **根因**：enum 载荷的绑定推迟到 arm 块内（tag 匹配后），但 guard 在“cond 阶段”求值，此时 `n` 尚未绑定。
- **修复**：JIT 把 enum arm 的 guard 求值移入 arm 块（载荷绑定后），内层 tag 判定与 guard 任一不满足都走 next_blk；LLVM 在 guard 求值前临时注入 pending_binds（求值后弹出）。
- **附带**：无载荷变体（如 `enum E { A(Result) C }` 的 `E::C`）曾因“cond 阶段预绑定”越界读载荷槽而崩溃（0xC0000005），一并修复。

### 43. bool 主体的 match 比较按 i64
- **现象**：`match true { true => 1; false => 0 }` —— JIT 正常，AOT 生成非法 IR（`icmp eq i64 true, true`，`true` 是 `i1`）。
- **根因**：`codegen::eq` 末分支一律 `icmp eq i64`，未处理 `Bool`。
- **修复**：`Bool` 主体用 `icmp eq i1`。

### 44. set 元素类型未从 insert 细化
- **现象**：`s := set()` 后 `insert(s, "hello")`，`for k in s { put(k) }` 输出 `1462139617280` 这类原始句柄值。
- **根因**：`sema_infer` 只对 `push`/`append`（list）细化元素类型，未处理 `insert`（set）→ 元素保持 `Unknown` → codegen 按 `i64` 取出。
- **修复**：`insert(set, x)` 同样细化 `Ty::Set` 的元素类型。

### 45. map 的 f64 值
- **现象**：`m := map(); m["k"] = 3.5; put(m["k"])` —— JIT 输出垃圾浮点（如 `6.95e-310`）；AOT 非法 IR（`double %t4` 实际是 `i64`）。
- **根因**：1) `map()` 的键/值类型未从 `m[k] = v` 细化（值类型保持 `Unknown`）；2) 容器以 i64 槽存储，f64 需按位保真存取，实现却用值转换。
- **修复**：`sema` 的 `IndexAssign` 对 `Ty::Map` 的键/值类型按首次赋值细化；容器存取改走“i64 槽 + f64 bitcast”。

### 46. list/map 的 f64 元素（统一修复）
- **现象**：`l := list(); push(l, 3.5); put(l[0])` —— JIT 垃圾浮点（`5.26e+83`）；AOT 非法 IR。
- **根因**：与第 45 同源 —— 容器元素槽是 i64，f64 元素必须 bitcast 保位模式，实现里存/取却是值转换。
- **修复**：JIT 新增 `to_slot`/`from_slot_jit`（f64↔i64 bitcast），`list_set`/`map_insert` 与 `Index` 的 List/Map 分支改用它们，`convert` 对 `(I64, F64)` 也 bitcast；LLVM 的 `Index` List/Map 分支元素为 F64 时用 `bitcast i64 <-> double`。


### 47. list 非整数下标 / for 非整数边界
- **现象**：`l["k"]`（list 用字符串下标）运行时崩 "index out of bounds"；`for i in 0..3.5` 死循环。
- **根因**：sema 未检查 list/array 下标与 for-range 边界的整数性。
- **修复**：sema 拒绝非整数下标与边界。

### 48. 元组类型 `-> (T, T)` 解析失败
- **现象**：`fn pair[T](a: T, b: T) -> (T, T)` 报 "类型 must be an identifier, found '('"。
- **根因**：`parse_type_inner` 无元组分支。
- **修复**：加 `(` 分支解析 `Ty::Tuple`。

### 49. 泛型函数前向引用
- **现象**：`fn a[T]` 调用定义在其后的 `fn b[T]` → `undefined function 'b'`。
- **根因**：mono 只对 `plain_fns` 做泛型调用改写，泛型实例之间的调用未改写。
- **修复**：对 `instances` 也做 `rewrite_calls`；`subst_block_ty` 实际替换体内类型。

### 50. `subst_ty_a` 缺 Tuple/Option/Ref
- **现象**：泛型 `Box[T]` 返回类型里的 `T` 未替换。
- **修复**：`subst_ty_a` 补 `Tuple`/`Option`/`Ref`/`RefMut`。

### 51. `where T: Ord` 对 int 报错
- **现象**：`fn max[T](...) where T: Ord` 用 `int` 实参报 "int does not satisfy bound"。
- **修复**：基础类型（int/f64/str/bool）内置满足 Ord/Eq/Hash 等。

### 52. 泛型函数名与变量同名
- **现象**：`fn a`/`fn b` 与 `max2` 的参数 `a`/`b` 同名时，`a > b` 被当成"闭包比较"。
- **根因**：`hoist::convert_fn_refs` 把裸 `Ident` 无条件转 `ClosureNew`，无遮蔽检查。
- **修复**：加局部绑定名集合，命中则不转。

### 53. dyn Trait 容器（list[dyn T]）
- **现象**：`list[dyn Trait]` 的 push 未装箱、for 迭代拿到裸指针、dyn 方法调用 ABI 不匹配。
- **修复**：sema 调用点细化 list 元素类型；mono 对 push/insert 自动装箱；codegen/jit 的 dyn data 用 ptr、for 元素 inttoptr。
---

## 统计

- 真实缺陷修复：**53 个**
- 测试：**626 → 825**（单元 25→143、双后端一致性 101→160、前端批量 500→522）
- fuzz/深挖用例：约 4000+，全部 `panic=0` 且双端一致
- 文档：`0.0.1d` 全量更新 + 文档示例逐条核对

---

# 第二轮：双后端一致性深挖（Bug Log · 续）

> 承接上文。本轮聚焦 **JIT（Cranelift）与 AOT（LLVM）逐字节一致** 的系统性验证，
> 覆盖全部标准库模块、全部语法糖、所有权/借用、高阶函数、运算符重载、泛型与综合场景。
> 基线：**825**（143 单元 + 522 前端批量 + 160 双后端一致性）；结束：**827**（166 一致性）。
> 所有修复均已提交并推送。

## 概览（本轮，共 12 项）

| # | commit | 类别 | 缺陷 | 现象 |
|---|---|---|---|---|
| 54 | `1e878fa` | codegen | `from_slot` 对 `Option`/`Result` 字段未 `inttoptr` | `a?.b`（`b` 为 `Option[T]` 字段）取到错误指针 |
| 55 | `8e8ac19` | codegen | `gt_list_*`/`gt_map_*` 声明类型不统一（i64 vs ptr）| list/map 同时被下标与 `put` 使用时 IR 重定义 |
| 56 | `f9b4c95` | codegen | `sb_push_f64` 传 `i64`（应为 `double`）| `str_builder` 追加浮点输出垃圾 |
| 57 | `aa1fc23` | JIT | 未注册 `sb_push_char` 运行时符号 | JIT 报 missing symbol |
| 58 | `a683649` | JIT | 未注册 `sb_pop` 运行时符号 | JIT 报 missing symbol |
| 59 | `f2a6cae` | sema | `expt e` 的 `e` 类型未定为 `Str` | `try{ f()? } expt e { put(e) }` 打印错误 |
| 60 | `e451df3` | codegen | AOT `return` 前未执行 `defer` | `fn f() { defer put("d") ... return }` AOT 缺 `d` |
| 61 | `8837a6a` | gtlib | `json_valid` 只查括号/引号配对 | `json_valid("bad")` 误返回 `true` |
| 62 | `60f3fa0` | mono/type | 链式运算符重载 `a + b + c` 失效 | `(a+b)` 是 `Call`，降级只认裸 `Ident`，输出句柄 |
| 63 | `e953e61` | type | 一元 `-` 对 `Struct` 返回 `Unknown`/`I64` | `-a + a`（运算符重载）输出句柄 |
| 64 | `24f9828` | codegen/JIT | 泛型 struct 显示带单态化后缀 | `Box$i` 应显示为 `Box` |
| 65 | `3f244ef` | codegen/JIT | `for-else` 的 `else` 从不执行 | `for … { } else { }` 的 `else` 被忽略 |

## 详细说明（本轮）

### 54. `from_slot` 对 Option/Result 字段未 inttoptr
- **现象**：`a?.b`（`b: Option[T]` 字段）在 AOT 下取到错误指针，JIT 正常。
- **根因**：`from_slot` 把 i64 槽还原为指针类型时，未对 `Option`/`Result` 字段做 `inttoptr`。
- **修复**：`from_slot` 对 `Option`/`Result` 生成 `inttoptr i64 -> ptr`。

### 55. gt_list_*/gt_map_* 声明类型不统一
- **现象**：list/map 同时被下标访问与 `put` 打印时，AOT 报 IR 符号重定义（`gt_list_len` 一处 `i64`、一处 `ptr`）。
- **根因**：不同调用点各自 `declare` 同名运行时函数，签名不一致。
- **修复**：统一 `gt_list_len/at`、`gt_map_len/key_at/val_at` 的声明为 `ptr`。

### 56. sb_push_f64 传参类型错误
- **现象**：`str_builder` 追加浮点后 `sb_finish` 输出垃圾。
- **根因**：`sb_push_f64` 用 `fptosi i64` 传值，运行时按 `double` 解释。
- **修复**：改为直接传 `double`。

### 57–58. JIT 缺失运行时符号
- **现象**：JIT 执行 `sb_push_char`/`sb_pop` 报 missing symbol。
- **根因**：`jit/symbol.rs` 的 `STDLIB_NAMES` 漏登记这两个符号。
- **修复**：补登记。

### 59. expt 绑定 e 的类型
- **现象**：`try { f()? } expt e { put(e) }` AOT 打印错误内容。
- **根因**：`expt` 绑定的 `e` 类型未定为 `Str`，`put` 按 i64 解释。
- **修复**：sema 将 `expt e` 的 `e` 定为 `Ty::Str`（与 sema 类型一致）。

### 60. AOT return 前未执行 defer
- **现象**：`fn f() { defer put("d1") ... return 1 }` AOT 缺 `d1`（JIT 有）。
- **根因**：AOT 的 `Stmt::Return` 未 drain `defer_stack`。
- **修复**：`return` 前逆序执行本函数已登记的 `defer`。

### 61. json_valid 校验过弱
- **现象**：`json_valid("bad")` 返回 `true`。
- **根因**：只做括号配对 + 引号闭合，未检查顶层值形态。
- **修复**：追加"顶层必须是合法 JSON 值"的检查（`{ [ " true false null 数字`），并拒绝裸标识符。

### 62. 链式运算符重载 a+b+c
- **现象**：`a + b + c`（`Vec`）JIT/AOT 输出句柄而非 `Vec {x: 6}`。
- **根因**：1) `binary_result` 对 `Vec + Vec` 返回 `Unknown`；2) `mono` 的运算符降级只认左操作数为裸 `Ident`，`(a+b)` 已是 `Call` 被漏掉。
- **修复**：`binary_result` 对两侧同 struct 返回该 struct；降级改用"左操作数类型名"（`vars` 或 `a.ty` 的 `Struct` 名）判断。

### 63. 一元 - 对 Struct 返回类型
- **现象**：`-a + a`（`Vec` 实现 `neg`）输出句柄。
- **根因**：`unary_result(Neg, Struct)` 报错/返回 `I64`，导致 `-a` 类型丢失。
- **修复**：`Neg` 对 `Struct` 返回该 `Struct`（`Vec__neg` 的结果类型）。

### 64. 泛型 struct 显示的 $i 后缀
- **现象**：`put(Box { v: 42 })` 显示 `Box$i {v: 42}`。
- **根因**：`put` 打印 struct 名时未剥离单态化后缀。
- **修复**：AOT/JIT 打印时去除 `$` 之后的后缀（`Box$i` → `Box`）。

### 65. for-else 的 else 从不执行
- **现象**：`for x in xs { } else { put("empty") }` 的 `else` 从不执行。
- **根因**：`ForEach` 的 codegen/JIT 均把 `els` 忽略（`els: _`）。
- **修复**：双端在循环 `exit` 块追加判定——索引等于长度（正常结束）才执行 `else`，`break` 时跳过。

### 66. JIT 的 defer 不执行（字符串未 intern）

- **现象**：`fn f() { defer put("d1") ... }` JIT 只输出 `body`，缺 `d1`（AOT 正确）。
- **根因**：`jit/call.rs` 的 `collect_strs_block` 缺 `Stmt::Defer` 分支 → `defer` 内字符串字面量未登记进数据段 → 生成期报 `string constant not interned`，被 `let _ = gen_expr` 静默吞掉。
- **修复**：`collect_strs_block` 增加 `Stmt::Defer(e, _) => collect_strs(e, out)`。
- **附带**：覆盖 void 函数末尾、`match` arm 的 `return`、多次 `defer` + `return` 三种路径。

### 67. enum 作 struct 字段/数组元素/Option 载荷时打印成 `Color {}`

- **现象**：`enum Color { Red, Green, Blue }`；`struct P { c: Color }`；`put(p.c)` 输出 `Color {}`（应为 `Color::Red`）。数组元素、`Option[Color]` 同理。
- **根因**：类型系统用 `Ty::Struct(name)` 表示**所有具名类型**（含 enum），无法区分。AOT 的 `emit_print_v` 落入 `Ty::Struct` 分支按结构体打印；此外数组元素加载后 `Val::new_slot` 丢了 `is_ptr` 标记，导致 `ptr` 值被当 `i64` 再 `inttoptr`（AOT 非法 IR）。
- **修复**：1) `emit_print_v`/`gen_print_v` 入口：若 `Ty::Struct(name)` 的 `name` 在 `enum_variants` 中，则按 `Ty::Enum` 打印；2) AOT `Ty::Array` 打印：`ptr` 类元素用 `Val::new_ptr` 保留标记。
- **验证**：`P {c: Color::Red}` / `[Color::Red, Color::Blue]` / `Some(Color::Red)` 双端一致。

### 68. 嵌套 Enum 解构 `E::Has(Shape::Circle(r))`

- **现象**：内层 enum 解构的绑定名 `r` 未定义（sema 报 E101）；即使绕过，内层变体 tag 判定错误（`E::Has(Shape::Rect(w,h))` 的 `w*h` 得 0 或垃圾）。
- **根因**：三处都只解一层——1) `sema_infer.rs` 的 enum 载荷绑定未递归内层 `EnumLit`；2) JIT `bind_pat_deep` 无 `EnumLit` 分支；3) AOT `bind_pat_deep` 无 `EnumLit` 分支。且"内层变体 tag"被硬编码为 0/1（只区分 None/Err），`Shape::Rect`（第 2 变体，tag=1）被误判。
- **修复**：三处补 `EnumLit` 递归绑定；内层变体 tag 从 `enum_variants` 查真实位置（`pat_tag_cond`/`want_inner`）。
- **验证**：`E::Has(Shape::Circle(2.0))`→12.56、`E::Has(Shape::Rect(3,4))`→12、`E::None`→0，双端一致。

### 69. `impl <trait> for <基础类型>`（int/str/f64/bool）

- **现象**：`impl Show for int { fn show(self) -> str { ... } }` —— `self` 被当作 `Ty::Struct("int")`，`str(self)` 报"str() 不能转换结构体(int)"；方法调用报 `MethodOn (non-dyn) not lowered`。
- **根因**：1) `hoist.rs` 把 `self` 一律设为 `Ty::Struct(ty)`；2) `mono.rs` 的 `MethodOn` 降级只认 `Ty::Struct`，基础类型无降级。
- **修复**：1) `self` 类型用 `Ty::from_name(ty)` 优先（int→I64 等），否则 `Ty::Struct`；2) `Ty` 新增 `impl_name()`（基础类型回映射到注解名），`mono` 的 `MethodOn` 对任意具名类型降级为 `类型__方法`。
- **验证**：`42.show()`→`int(42)`、`"hi".show()`→`str(hi)`，双端一致。
- **未覆盖**：泛型 blanket impl（`impl[T] Show for T`）仍报 "missing method"（见待办）。

### 70. `fily`（finally）在 `return` 时不执行

- **现象**：`fn f(x:int)->int { try { return g(x) } expt e { return -1 } fily { put("finally") } }` 不打印 `finally`（JIT/AOT 都错）。且 sema 先报"函数缺返回值"。
- **根因**：1) `sema.rs` 的 `block_yields`/`infer_block_ret` 未识别 `Stmt::Try`（`try` 内的 `return` 不算返回值路径）；2) codegen/JIT 的 `fily` 只挂在 try 的 `l_end` 块，而 `return` 直接 `ret`，`l_end` 无前驱 → `fily` 被优化掉。
- **修复**：1) `block_yields`/`infer_block_ret` 增加 `Try`（body/catches 递归）；2) codegen/JIT 各加 `fily_stack`：进入 try body / catch body 时 push `fin`，`return` 时逆序执行栈内所有 `fily`。
- **验证**：`f(5)`→finally/10，`f(-1)`→caught: neg/finally/-1，双端一致。

### 71. `ast` 库 AOT 产物依赖 `ast.dll`（缺 `ast_static.lib`）

- **现象**：`import ast; put(ast_dump(ast_lit_int(42)))` 用 `--c` 编译后运行报 `3221225781`（STATUS_DLL_NOT_FOUND）。
- **根因**：`build.bat` 对 C 语言 gtlib 模块（`ast.c`）只产出 `.dll`（`-shared`），不产出 `_static.lib`；`find_std_libs_for` 优先找 `ast_static.lib`（不存在）→ 回退到 `ast.lib`（dll 导入库）→ 产物依赖 `ast.dll`。
- **修复**：`build.bat` 的 C 模块循环追加 `lld-link /lib` 产出 `<mod>_static.lib`。另将 `ast.c` 的 `int op` 形参改为 `int64_t op`（与 GTLang 侧 `i64` 调用约定一致，更稳妥）。
- **验证**：`ast_lit_int`/`ast_binary`/`ast_dump`/`ast_free` 的 AOT 产物独立运行，与 JIT 一致。

### 72. `match` 表达式返回定长数组

- **现象**：`a := match x { 1 => { [1,2,3] } _ => { [4,5] } }` JIT 输出句柄、AOT 生成非法 IR（`store i64 %v2` 但 `%v2` 是 `ptr`）。
- **根因**：`type_join` 对 `Ty::Array` 无分支 → 两个 arm 的 `Array` 类型 join 成 `Unknown` → `match` 表达式类型为 `Unknown` → codegen 的槽类型退化为 `i64`，而 arm 返回 `ptr`。
- **修复**：`type_join` 增加 `Ty::Array`（逐元素 join，长度取前者）。
- **验证**：`[1, 2, 3]` 双端一致。

## 补充：已知限制（非本轮修复）

- **闭包捕获数不可知**：`fn apply(f, x) { f(f(x)) }` 里 `f` 是"高阶函数形参"，其 `Ty::Closure` 的显式参数列表为空，codegen 无法得知该闭包"前置捕获值"的个数（`ncap=0`），导致"捕获闭包经高阶函数间接调用"时捕获值未传递。直接调用（`triple(5)`）正常。
- **闭包按值捕获**：闭包内修改外层变量不影响外层（设计如此）。
- **map 的 struct 键**：按指针（句柄）比较，非内容。
- **泛型函数返回泛型 struct**：返回的 `Box[T]` 字段类型未单态化。
- **嵌套泛型** `Box[Box[T]]`：字段类型冲突。
- **`a.b[i] = v`**（字段-下标赋值）：语法缺口。
- **插值 `$p.x`**：应写 `${p.x}`。
- **`?.` 链式（中间字段是 Option）**：会嵌套 `Option`。
- **blanket impl** `impl[T] Trait for T`：未支持。
- **enum `==` 只比较 tag**：带载荷的 enum（如 `Msg::Text("a") == Msg::Text("b")`）会因 tag 相同而误判相等；未逐载荷比较。
- **`Option[T] == Option[T]` / `Result == Result`**：不支持（`cannot compare`）；`list`/`map`/`set` 同理未支持 `==`。
- **enum 的 `< > <= >=`（`@derive(Ord)`）**：不支持；`@derive` 只对 struct 生效（enum 仅 `Debug` 生成 `to_str`）。
- **`self.字段.方法()`**（在 impl 方法内直接对"字段"调其类型的方法）：会被误改写成 `本类型__字段.方法`；变通：`tmp := self.字段; tmp.方法(...)`。
- **enum 的 `impl` 方法**（`impl Color { fn code(self) }`）：`c.code()` 报 undefined function；enum 上的方法调用未降级。
- **`//` 行尾注释**：因与整除运算符 `//` 消歧，部分位置（如 `put(x) // 注释`）不被识别；推荐用 `#` 注释。
- **显式 `&self` 方法**（`fn m(self: &T)`）：调用 `v.m()`（v 是 T）不会自动借用，报"须 &T"；变通：写 `self` 或 `self: T`。
- **`&mut 字段` 的写回**：`ry := &mut p.y; ry = 99` 不会写回 `p.y`（借用运行时透传值）；变通：直接 `p.y = 99`。
- **列表推导式的"字符串/复杂表达式元素"**：`["n" + str(x) for x in xs]` / `[str(x) for x in xs]` 的元素类型未传导到 codegen（`gt_list_new(0)` → 按 i64），打印为句柄；`[x*2]` / `[x]`（数值）正常。
- **`map` 的 enum 键**：按指针（句柄）比较，非内容（与 struct 键同）。
- **`@derive(Eq)` 对 enum 不生成 `eq`**（仅生成 `to_str` 的 Debug）；enum 的 `==` 靠 codegen 的 tag 比较。

### 73. 泛型 struct 多实例区分（Pair[i64,str] vs Pair[str,i64]）

- **现象**：`struct Pair[A,B]`；`Pair{first:1,second:"x"}` 与 `Pair{first:"k",second:9}` —— 后者被误识别为前者的实例 `Pair$i_s`，报 "field must be i64"。
- **根因**：`mono::rewrite_struct_lit_expr` 用"discriminant 集合"匹配实例（`Pair$i_s` 与 `Pair$s_i` 的集合都是 `{I64,Str}`），取第一个 → 误选。
- **修复**：改为按结构体字段顺序"逐位精确匹配"（`tys[i]` 对 `fields[i].ty` 的 discriminant）。

### 74. 用户泛型类型多参数 `Pair[A,B]` 解析失败

- **现象**：`fn f(p: Pair[A, B])` 报 "expected ']', found ','"。
- **根因**：`parser_type` 的 `_`（用户泛型）分支只解析一个类型参数，不消费后续 `,`。
- **修复**：`_` 分支 `while eat(",") { parse_type() }` 消费完多余类型参数（由 mono 从字面量推导）。

### 75. `??` 链式左结合导致 AOT 非法 IR

- **现象**：`a ?? b ?? c` —— JIT 正确，AOT 报 `store i64 %t30`（`%t30` 是 `ptr`）。
- **根因**：`??` desugar 为 `match a { Some(v)=>v, None=>b }`；左结合时内层 `(a??b)` 的 arm 类型（`v` vs `b`）join 成 `Unknown` → 外层槽退化 `i64`。
- **修复**：`??` 改为**右结合**（`a ?? (b ?? c)`），内层 `??` 结果为标量类型。

## 统计（第二轮）

- 真实缺陷修复：**22 个**（累计 **75 个**）
- 测试：**825 → 827**（单元 143、前端批量 522、双后端一致性 160→166）
- 验证方式：对同一 `.gt` 分别跑 `gtc --run`（JIT）与 `gtc --c`（AOT 产物），断言 stdout **逐字节一致**
- 覆盖：全类型 `put`、嵌套容器、全部语法糖/匹配、全部标准库模块、所有权/借用、高阶函数/闭包、运算符重载、泛型、CLI、大数/浮点/递归/位运算/短路/循环控制，以及综合场景（学生管理、栈式求值器、矩阵、单词计数、斐波那契记忆化、LRU 缓存、优先队列）

## 待办（已知未修）

| 优先级 | 缺陷 |
|---|---|
| 高 | JIT 的 `defer` 在 `fn` 末尾 / `match` arm 的 `return` 前不执行（AOT 正确）|
| 中 | `enum` 作 struct 字段类型时打印 `Color {}` |
| 中 | 一行嵌套 Enum 解构 `E::Has(Shape::Circle(r))` |
| 中 | blanket impl / 泛型 impl 方法调用 |
| 中 | `fily` 块在 `expt` 里 `return` 时不执行 |
| 低 | `ast` 库 AOT 链接（缺 `_static.lib`）|
| 低 | `map` 的 struct 键（按指针比较，非内容）|
| 低 | 泛型函数返回泛型 struct 时字段类型未单态化 |
| 低 | 嵌套泛型 `Box[Box[T]]` 字段类型冲突 |
| 低 | `match` 表达式返回定长数组 |
| 低 | 插值 `$p.x`（应写 `${p.x}`）|
| 低 | `?.` 链式（中间字段本身是 `Option`）|
| 低 | `a.b[i] = v`（字段-下标赋值，语法缺口，改动面 ~12 处）|

