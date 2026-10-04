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

---

## 统计

- 真实缺陷修复：**38 个**
- 测试：**626 → 814**（单元 25→142、双后端一致性 101→149、前端批量 500→522）
- fuzz/深挖用例：约 2500+，全部 `panic=0` 且双端一致
- 文档：`0.0.1d` 全量更新 + 42 个文档示例逐条核对
