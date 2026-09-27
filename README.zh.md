<div align="center">

<img src="GTlangLOGO.png" alt="GTLang" width="200">

# GTLang

**静态类型、编译型、表达式导向的编程语言，原生支持中文标识符。**

[![Tests](https://img.shields.io/badge/tests-613%20passed-brightgreen)]()
[![Backends](https://img.shields.io/badge/backends-LLVM%20%2B%20Cranelift-blue)]()
[![Warnings](https://img.shields.io/badge/warnings-0-brightgreen)]()
[![License](https://img.shields.io/badge/license-MIT-lightgrey)]()

中文 | [English](README.md)

</div>

---

## 目录

- [GTLang 是什么？](#gtlang-是什么)
- [亮点](#亮点)
- [快速上手](#快速上手)
- [代码一览](#代码一览)
- [语言特性](#语言特性)
- [双后端](#双后端)
- [性能](#性能)
- [CLI 速查](#cli-速查)
- [目录结构](#目录结构)
- [架构](#架构)
- [测试](#测试)
- [文档](#文档)
- [常见问题](#常见问题)

---

## GTLang 是什么？

GTLang 是一门**静态类型**、**编译型**、**表达式导向**的编程语言，融合了：

- **Rust 的内存安全** —— 所有权、借用、流敏感 NLL、边界/溢出/除零检查
- **C 级性能** —— 经 LLVM 生成原生机器码；紧循环可与 C 持平
- **Python 式简洁** —— 表达式导向、类型标注可选、闭包、列表推导
- **原生中文标识符** —— \`计数器\`、\`累加\`、\`点\` 都是合法名字
- **双后端** —— 同一份 AST 既可编译为原生可执行文件（LLVM），也可内存执行（Cranelift JIT），**输出逐字节一致**（测试保证）

---

## 亮点

| | 特性 |
|---|---|
| 🧠 | **静态类型** + 双向推断 |
| 🛡️ | **内存安全** —— 所有权、借用、NLL、边界/溢出检查 |
| ⚡ | **双后端** —— LLVM（发布）+ Cranelift（JIT），语义一致 |
| 🌏 | **中文标识符** —— 无需音译 |
| 🧬 | **泛型**（单态化）、trait、**dyn Trait**（vtable 动态分发） |
| 🎯 | **带载荷枚举** + 穷尽 \`match\`（范围/守卫/OR） |
| 🚀 | **并发** —— \`go\`（真线程）+ \`chan\`（无界通道） |
| 🔌 | **C 交互** —— 内联 C、\`extern "C"\`、\`import c "头.h"\` |
| 🧩 | **宏** —— 声明式 \`macro\` + \`@derive(Eq, Clone, Debug, ...)\` |
| 💬 | **智能诊断** —— 稳定错误码、中英双语、"是否想用 X？" |
| 📦 | **模块** —— \`import math\`（内置）、\`import a.b\`（用户）、\`import "x.gt"\` |
| 🛠️ | **工具链** —— \`gtc\`（编译器/解释器）、\`gtfmt\`（格式化器） |

---

## 快速上手

### 构建

```bat
REM 需要 PATH 上有 Rust + LLVM/clang（TCC 可选，用于内联 C）
REM 可用 GTC_CLANG / GTC_TCC 覆盖自动探测
build.bat           REM 发布构建
build.bat debug     REM 调试构建
build.bat test      REM 运行测试
build.bat clean     REM 清理产物
```

产物：
- \`target\release\gtc.exe\` —— 编译器 & 解释器
- \`target\release\gtfmt.exe\` —— 格式化器
- \`res\lib\*.dll\` —— 标准库

### 运行第一个程序

创建 \`hello.gt\`：

```gt
fn main() {
    put("你好，世界！")
}
```

然后：

```bat
REM 直接运行（Cranelift JIT）
target\release\gtc.exe --run hello.gt

REM 编译为独立可执行文件（LLVM + clang）
target\release\gtc.exe hello.gt -o hello.exe -O 2
hello.exe
```

---

## 代码一览

```gt
// 结构体、枚举、trait、泛型、模式匹配、闭包、并发
@derive(Debug, Clone, Eq)
struct 点 { x: int, y: int }

enum 形状 {
    Circle(f64)
    Rect(f64, f64)
    Unit
}

trait 面积 {
    fn area(self) -> f64
}

impl 面积 for 形状 {
    fn area(self) -> f64 {
        match self {
            形状::Circle(r) => { return 3.14159 * r * r }
            形状::Rect(w, h) => { return w * h }
            形状::Unit => { return 0.0 }
        }
    }
}

// 泛型函数（单态化）
fn 映射[T](xs: list, f) -> list {
    r := list()
    for x in xs { push(r, f(x)) }
    return r
}

fn main() {
    p := 点 { x: 3, y: 4 }
    put(p.to_str())                    // 点 { x: 3, y: 4 }

    // 列表推导
    平方 := [x * x for x in 0..6]
    put(len(平方))                      // 6

    // 计数循环
    loop 3 { put("hi") }

    // 穷尽匹配
    s := 形状::Circle(2.0)
    put(s.area())                       // 12.56636

    // 真线程 + 通道
    ch := chan()
    go 生产者(ch)
    put(chan_recv(ch))                  // 42
}

fn 生产者(ch) {
    chan_send(ch, 42)
}
```

---

## 语言特性

### 变量

```gt
a = 1          // 裸赋值：自动声明 + 推导
x := 2         // 推导声明（类型可变）
let mut y = 3  // 推导，可变
let n: int = 4 // 显式固定类型
const K = 5    // 常量
```

### 控制流

```gt
if c { ... } elif c2 { ... } else { ... }
loop 3 { ... }              // 计数循环
while c { ... }
do { ... } while c
for i in 0..n { ... }
for v in 容器 { ... }
for ... else { ... }        // 无 break 时执行 else
outer: for ... { break outer }

match v {
    1 => { ... }
    1..10 => { ... }        // 范围
    1 | 2 | 3 => { ... }    // OR
    n if n > 0 => { ... }   // 守卫
    _ => { ... }            // 通配
}
```

### 函数

```gt
fn 加(a: int, b: int) -> int { a + b }   // 尾表达式即返回值

fn f(a, b) { ... }                        // 类型可推断
fn g(x: int = 10) { ... }                 // 默认参数
g(x: 5)                                   // 命名参数

fn 恒等[T](x: T) -> T { x }              // 泛型
fn max[T](a: T, b: T) -> T where T: Ord { ... }  // where 约束

|x| x * 2                                 // 闭包
|x: int| -> int { x * x }                 // 标注闭包
```

### 结构体 / 枚举 / trait

```gt
struct 点 { x: int, y: int }

enum 形状 { Circle(f64) Rect(f64, f64) Unit }

trait 面积 { fn area(self) -> f64 }
impl 面积 for 圆 { fn area(self) -> f64 { ... } }

fn print_area(s: dyn 面积) { put(s.area()) }  // 动态分发

@derive(Eq, PartialEq, Clone, Debug, Display, Default, Hash, Ord)
struct 点 { x: int }
```

**可用的 \`@derive\`：**

| 派生 | 生成 |
|---|---|
| \`Eq\` | \`eq\`、\`ne\` |
| \`PartialEq\` | \`eq\` |
| \`Clone\` | \`clone\` |
| \`Debug\` | \`to_str\` → \`"类型名 { f: v }"\` |
| \`Display\` | \`to_str\` → \`"v1, v2"\` |
| \`Default\` | \`default\` |
| \`Hash\` | \`hash\` |
| \`Ord\` | \`cmp\`、\`lt\`、\`le\`、\`gt\`、\`ge\`（+ \`<\`、\`<=\`、\`>\`、\`>=\`） |

### 运算符重载

```gt
impl 向量 {
    fn add(self, o: 向量) -> 向量 { ... }
    fn lt(self, o: 向量) -> bool { ... }
}
a + b    // → 向量__add(a, b)
a < b    // → 向量__lt(a, b)
```

### 并发

```gt
go 工作者(42)          // 启动真线程
ch := chan()           // 无界通道
chan_send(ch, v)
v := chan_recv(ch)     // 阻塞接收
sleep(500)             // 毫秒
```

### C 交互

```gt
C {
    static long long 平方(long long x) { return x * x; }
}
put(平方(5))           // 自动解析签名

extern "C" { fn puts(s: str) -> int }

import c "math.h" as m
put(m.sqrt(2.0))
```

### 错误处理

```gt
fn f(n: int) -> Result[int, str] {
    if n < 0 { return Err("负数") }
    return Ok(n * 2)
}

v := f(21)?                                     // ? 传播
match f(-1) { Ok(v) => { ... } Err(e) => { ... } }

o := Some(1)
v := o or 0                                     // 默认值

try { throw "异常" } expt e { put(e) } fily { ... }
```

### 数据扩展

```gt
t := (1, 2)                // 元组
let (a, b) = t             // 解构
s[0..3]                    // 切片
l[-1]                      // 负索引
a, b = b, a                // 交换
[x * 2 for x in l if x > 0]  // 列表推导
```

---

## 双后端

GTLang 提供**两个后端**，消费同一份 AST：

| | \`--c\`（LLVM） | \`--run\`（Cranelift） |
|---|---|---|
| **产物** | 独立原生可执行文件 | 内存中 |
| **编译速度** | 慢（clang） | 快（JIT） |
| **运行速度** | 快（优化充分） | 基线 |
| **适用** | 发布 / 分发 | 开发 / 脚本 |
| **一致性** | — | **输出逐字节一致** |

一致性由 \`tests/consistency.rs\` 强制：每个测试用同一份源码跑两个后端，断言 stdout 相同。

---

## 性能

基准：\`loop_sum(2e8)\` —— 累加 \`0..200_000_000\`，**开启溢出检查**。

| 实现 | fib(35) | loop_sum(2e8) |
|---|---:|---:|
| C（clang -O2） | ~26 ms | ~5.5 ms |
| Rust（rustc -O） | ~28 ms | ~8 ms |
| **GTLang（-O2，有检查）** | ~40 ms | **5.9 ms** |
| Node.js 24（V8 JIT） | ~144 ms | ~195 ms |
| Lua 5.4 | ~445 ms | ~673 ms |
| Python 3.12 | ~1300 ms | ~6788 ms |

**GTLang 默认（安全）模式在紧循环上追平 C** —— 流敏感范围分析（\`src/range.rs\`）证明安全即省略溢出检查，**不牺牲安全性**。

详见 [doc/bench.md](doc/bench.md)。

---

## CLI 速查

```text
gtc --c        <文件.gt> [...] [-o 输出] [-O 0..3]   编译为可执行文件
gtc --run      <文件.gt> [...]                        解释执行（Cranelift JIT）
gtc --check    <文件.gt> [...]                        仅检查（词法/语法/类型）
gtc --lint     <文件.gt> [...]                        静态检查（未用函数/变量/参数、不可达代码等）
gtc --lint --strict <文件.gt>                         警告视为错误
gtc --lint --json   <文件.gt>                         JSON 输出（供 CI）
gtc --emit-llvm <文件.gt> [...]                       仅生成 LLVM IR
gtc --test     <文件.gt>                              运行 test_ 前缀的测试
gtc --watch/-w <文件.gt> ...                          监控变更自动重跑
gtc --version / --verbose                             版本 / 详细日志
gtc ... zh                                            中文诊断

gtfmt [--check] <文件.gt>                             格式化（先经完整检查）
```

---

## 目录结构

```text
src/
  lib.rs            模块声明 + 对外 API
  main.rs           CLI（参数解析 / 诊断渲染）
  bin/gtfmt.rs      格式化器
  lint.rs           静态检查（供 gtc --lint）
  ast.rs            统一 AST
  lexer.rs          词法（中文标识符、字符串插值、原始串）
  parser.rs         递归下降语法
  parser_recover.rs 语法错误恢复（panic-mode）
  sema.rs           语义 / 类型检查
  type.rs           类型系统（单一事实来源）
  codegen/          LLVM 后端（mod / expr / call）
  jit/              Cranelift 后端（mod / stmt / expr / call / rt / symbol）
  own.rs            所有权 / 借用检查（流敏感 NLL）
  cblock.rs         内联 C 提取 + 桥接 + C 头解析
  tcc.rs            libtcc 动态绑定
  driver.rs         工具链定位 + clang 调用
  module.rs         import 模块系统 + import c
  hoist.rs          嵌套函数 / 闭包提升 + impl/trait 展平
  mono.rs           泛型单态化 + 方法降级 + where 约束校验
  opt.rs            优化 pass + 宏展开 + 列表推导展开
  range.rs          整数范围分析（省略冗余溢出检查）
  unit.rs           前端产物 Unit
  diag.rs           诊断类型 + 源码位置
  encoding.rs       源文件解码
  tmp.rs            临时目录
  lang.rs           全局语言开关 + 双语消息
  runtime/gt_rt.c   内置 C 运行时（并发、通道、容器）
  stdlib/           标准库源码（math.rs / string.rs）
res/lib/            标准库产物（math.dll / string.dll + .lib）
examples/  tests/  bench/
toolchain/          （可选）vendored Rust + LLVM + TCC
```

---

## 架构

```text
源码文本
   │  cblock::extract（挖空内联 C）
   ▼
lexer → parser → AST
   │  module::Linker（import 解析，含 import c）
   ▼
hoist（嵌套函数/闭包提升，impl/trait 展平）
   │  opt.resolve_named + opt.apply_defaults
   │  opt.expand_list_comp + opt.expand_macros
   │  expand_derives（@derive）
   ▼
sema(1)（填调用点类型）
   │  mono（方法降级 + 泛型单态化 + 运算符重载）
   │  opt.inline_and_fold + sema(2) 重推断
   ▼
sema(2)（完整类型检查）
   │  own（所有权/借用，NLL）
   │  opt.dead_code
   ▼
Unit ──┬── jit::run（Cranelift）
       └── codegen（LLVM IR → clang）
```

---

## 测试

```bat
cargo test --release
```

**613 个测试**：
- **17 个单元测试**（\`--lib\`）—— 类型系统、cblock、tmp
- **96 个双后端一致性测试**（\`tests/consistency.rs\`）—— 同一源码、两个后端、stdout 相同
- **500 个前端批量测试**（\`tests/bulk.rs\`）—— parse + type-check 覆盖

---

## 文档

| 文档 | 内容 |
|---|---|
| [doc/LANGUAGE.md](doc/LANGUAGE.md) | 语言手册（17 章） |
| [doc/PERFORMANCE.md](doc/PERFORMANCE.md) | 性能 |
| [doc/COMPARISON.md](doc/COMPARISON.md) | GTLang vs Python（摘要） |
| [doc/GTLang_vs_Python.md](doc/GTLang_vs_Python.md) | GTLang vs Python（详细） |
| [doc/bench.md](doc/bench.md) | 基准 |
| [doc/syntax_status.md](doc/syntax_status.md) | 语法 / 特性状态 |

所有文档均有**中文**（\`doc/\`）与**英文**（\`doc/en/\`）两个版本。

---

## 常见问题

**Q：为什么支持中文标识符？**
A：GTLang 把 Unicode 标识符视为一等公民。\`计数器\`、\`累加\` 与 \`counter\`、\`sum\` 同样合法，便于中文开发者与领域命名（数学、几何），无需音译。

**Q：为什么有两个后端？**
A：JIT（Cranelift）提供开发时的即时反馈；LLVM 后端产出优化的原生二进制用于发布。两者消费同一 AST 且输出一致 —— 测试保证。

**Q：性能为何接近 C？**
A：溢出检查是唯一成本。流敏感范围分析在可证明安全时省略检查（如 \`for i in 0..N { s += i }\`），让 LLVM 向量化循环。详见 \`doc/bench.md\`。

**Q：有垃圾回收吗？**
A：暂无。容器是引用语义，存活至进程退出。RC 原语（\`gt_rc_inc\` / \`gt_rc_dec\`）已就绪，插桩待续。

**Q：有哪些未实现？**
A：内联汇编（已移除）、\`async/await\`（与 \`go\` 重复）、完整 GC 插桩、交叉编译、过程宏。详见 [doc/syntax_status.md](doc/syntax_status.md)。

---

## 许可证

MIT（如存在 [LICENSE](LICENSE)）。
