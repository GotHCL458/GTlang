# GTLang 语法 / 特性总览

> [English](en/syntax_status.md)

> 版本 0.0.1d ｜ 双后端（LLVM + Cranelift）逐字节一致

---

## 1. 已实现（100%）

### 1.1 词法
- 中文/Unicode 标识符、`//` 与 `#` 注释、`///` 文档注释
- 整数（`0x/0b/0o/_`）、浮点、字符串插值 `$X`/`${expr}`、原始串 `r"..."`

### 1.2 变量
`:= ` / `let` / `let mut` / `const` / **裸赋值 `a = v`**（自动声明）；推导可变、显式固定

### 1.3 控制流
`if/elif/else`、`while`、`do-while`、`for i in a..b`、`for v in 容器`、`for-else`、
**标签循环** `outer: for { break outer }`、`break/continue`、
`match`（字面量/通配/守卫/**范围**/**OR**/**裸标识符绑定**/**嵌套解构**）、**成员 `in`**、**下标糖**、**`++`/`--`**

### 1.4 函数
参数/返回类型可省略、中文名、递归、**嵌套函数**、泛型（单态化 + **where 约束**）、
闭包、**默认参数**、**命名参数**、**trait 默认方法**、**闭包标注** `|x: int| -> int`、
**函数作一等值**（顶层 fn 名可传参/存变量/放 `if`/`match`/`return`/容器）、
**任意表达式作 callee**（`fs[0](5)`、`(f)(10)`）、**高阶函数**（无标注闭包参数）、
**函数返回闭包**（`return |x| x * n`）、**字符串 Unicode 转义**

### 1.5 类型
基础、定长数组、list/set/map、struct、泛型 struct、trait+impl（含 blanket）、
借用 `&T`/`&mut T`（**字段/方法/参数自动解引用**）、**元组**、**枚举**、**`dyn Trait`**（vtable 动态分发 + **struct 实参自动装箱**）

### 1.6 数据扩展
元组 `(1,2)`（`t.0`）、解构 `let (a,b) = t`、切片 `s[0..5]`、负索引 `l[-1]`、
解包交换 `a,b = b,a`、列表推导 `[x*2 for x in l if c]`

### 1.7 运算符重载
`impl T { fn add/sub/mul/div/rem/eq/ne/lt/le/gt/ge/neg }` → `a + b` / `-a`

### 1.8 宏
声明式宏 `macro 名(参数) { 模板 }`；`@derive`：**Eq / PartialEq / Clone / Debug / Display / Default / Hash / Ord**

### 1.9 并发
`go f(args)`（真线程）、`chan()`/`chan_send`/`chan_recv`（无界通道）、`sleep(ms)`

### 1.10 模块
`import math`（库，无引号）、`import a.b`（用户模块）、`import "x.gt"`（文件）、`import c "a.h"`（C 头）

### 1.11 C 交互
内联 C `C { ... }`、`extern "C"`、`import c "a.h"`

### 1.12 错误处理
Result/Option + `?`、`try/expt/fily` + `throw/raise`、`expr or 默认值`（Option 与 Result 均支持）

### 1.13 所有权
move 语义、借用冲突（`&mut` 独占）、**流敏感 NLL**（last_use + 分支 join + 不可达）

### 1.14 内存安全
边界检查、除零检查、**加/减/乘溢出检测**

### 1.15 诊断
稳定错误码（E001–E8xx）+ Help 建议 + ariadne 定位 + 双语 + **多错误**

### 1.16 优化
内联 + 常量折叠 + 常量传播 + 死代码消除 + 范围分析（省略冗余边界检查）

### 1.17 类型推断
局部双向推断（`let x: T = 值`）、`match` arm 结果 `type_join`、调用点迭代推断

### 1.18 `match` 穷尽性
enum 列全 / bool 需 true+false / Option 需 Some+None / Result 需 Ok+Err（或 `_`）；整数/浮点/字符串主体始终要求 `_`

### 1.19 内置函数
range、assert、三元、pad_left/right/fmt_int、put/len/str/int/f64/bool、
容器操作、字符串操作（split/join/upper/lower/trim/find/substr/replace/repeat）、
mem_*（裸内存）、chan/sleep

---

## 2. 工具链

| 工具 | 功能 |
|---|---|
| `gtc` | 编译器/解释器（--c/--run/--check/--lint/--emit-llvm/--test/--watch） |
| `gtc --lint` | 静态检查（未用函数/变量/参数、不可达、空 if、常量条件、自比较） |

**统一**：`-h`/`--help` + 末尾 `zh`（双语帮助）

---

## 3. 未实现 / Not Planned

| 项 | 说明 |
|---|---|
| 内联汇编 | 已移除（双后端一致性优先） |
| `async/await` | 与 `go` 重复，复杂度高 |
| 完整 GC 插桩 | RC 基础已就绪，插桩待续 |
| 跨平台交叉编译 | 当前 Windows |
| 过程宏（编译期代码生成） | 当前仅声明式宏 + `@derive` |
| HM 全推断 | 当前局部推断 |

---

## 4. 测试

**814 测试**（142 单元 + 149 双后端一致性 + 522 前端批量），`cargo test --release` 全绿，**0 warning**。


