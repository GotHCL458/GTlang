# GTLang 语言手册 / Language Reference

> [English](en/LANGUAGE.md)

> 版本：0.0.1d ｜ 编译器：gtc（双后端）｜ 编码：UTF-8（自识别）

---

## 1. 词法 / Lexical

| 元素 | 语法 | 说明 |
|---|---|---|
| 注释 | // … 、 # … | 行注释 |
| 文档注释 | /// … | 供文档提取 |
| 标识符 | 字母/下划线/任意 Unicode（含中文） | 累加、计数器、x |
| 整数 | 123、0xFF、0b1010、0o17、1_000 | 归一为 i64 |
| 浮点 | 3.14、1.5e-3 | f64 |
| 布尔 | true、false | bool |
| 字符串 | "..." | 插值：美元名 / 美元花括号表达式 |
| 原始串 | r"..." | 不转义、不插值 |

---

## 2. 变量 / Variables

    a = 1          // 裸赋值：自动声明 + 推导（可改类型）
    x := 2         // 推导声明
    let mut y = 3  // 推导，可变
    let n: int = 4 // 显式固定类型（不可改类型）
    const K = 5    // 常量

| 声明 | 类型 | 可改类型 |
|---|---|---|
| a = v | 推导 | 可 |
| x := v | 推导 | 可 |
| let mut y | 推导 | 可 |
| let n: T | 显式 | 不可 |
| const K | 固定 | 不可 |

---

## 3. 类型 / Types

| 类型 | 写法 | 语义 |
|---|---|---|
| 整数 | int（i64） | 值 |
| 浮点 | f64 / float | 值 |
| 布尔 | bool | 值 |
| 字符串 | str / string | 堆（NUL 结尾） |
| 定长数组 | [T; N] | 值（栈） |
| 列表 | list / List[T] | 引用 |
| 集合 | set | 引用 |
| 映射 | map / dict | 引用 |
| 元组 | (T1, T2) | 值（堆块，t.0） |
| 结构体 | struct 名 { ... } | 值（栈） |
| 枚举 | enum 名 { V1(T) V2 } | 引用（堆块 [tag, payload]） |
| 结果 | Result[T, E] | 引用 |
| 可选 | ?T / Option[T] | 引用 |
| 借用 | &T / &mut T | — |
| trait 对象 | dyn Trait | 引用（vtable） |
| 闭包 | 竖线x竖线 … | 值（[fn_ptr, caps]） |

---

## 4. 控制流 / Control Flow

    if c { ... } elif c2 { ... } else { ... }
    loop 3 { ... }             // 计数循环（重复 3 次；也可用变量/表达式）
    while c { ... }
    do { ... } while c
    for i in 0..n { ... }        // 范围
    for v in 容器 { ... }         // 迭代
    for ... else { ... }          // 无 break 时执行
    outer: for ... { break outer }  // 标签循环

    match v {
        1 => { ... }
        1..10 => { ... }          // 范围
        1 | 2 | 3 => { ... }      // OR
        n if n > 0 => { ... }     // 守卫
        _ => { ... }              // 通配
    }

穷尽性检查：enum 需列全变体 / bool 需 true+false / Option 需 Some+None / Result 需 Ok+Err（或 _ 兜底）。

---

## 5. 函数 / Functions

    fn 加(a: int, b: int) -> int { a + b }
    fn f(a, b) { ... }                          // 类型可省略（推断）
    fn g(x: int = 10) { ... }                   // 默认参数
    g(x: 5)                                     // 命名参数
    fn 恒等[T](x: T) -> T { x }                // 泛型
    fn max[T](a: T, b: T) -> T where T: Ord { ... }  // where 约束
    竖线x竖线 x * 2                                 // 闭包
    竖线x: int竖线 -> int { x * x }                 // 闭包标注

### 闭包与一等函数 / Closures & first-class functions

    // 闭包捕获外层变量
    n := 10
    f := |x: int| x + n
    f(5)                       // 15

    // 函数作一等值：顶层函数名可直接当值用
    fn 加一(x: int) -> int { x + 1 }
    g := 加一                  // 存变量
    g(10)                      // 11
    应用(加一, 5)              // 作实参（高阶函数）
    选 := if true { 加一 } else { 乘二 }   // if/match 分支
    fs := list()               // 放进容器
    push(fs, 加一)
    fs[0](5)                   // 任意表达式作 callee

    // 函数返回闭包
    fn 造乘(n: int) { return |x: int| x * n }
    m3 := 造乘(3)
    m3(10)                     // 30

    // 高阶函数：无标注的闭包参数会被推断为闭包
    fn 折叠(f, init: int, xs: list) -> int {
        acc := init
        i := 0
        while i < len(xs) { acc = f(acc, xs[i])  i = i + 1 }
        return acc
    }
    折叠(|a: int, b: int| a + b, 0, xs)

> 注：闭包体里对"被捕获的闭包参数"的调用会降级为间接调用。

### 借用自动解引用 / Ergonomic borrows

    struct 点 { x: int, y: int }
    p := 点 { x: 1, y: 2 }
    r := &p
    r.x                      // 等价于 p.x（字段访问自动解引用）
    r.y = 9                  // &mut 亦可写字段
    m := &mut p
    m.x = 10

    trait 形状 { fn 面积(self) -> int }
    impl 形状 for 圆 { fn 面积(self) -> int { ... } }
    fn f(s: dyn 形状) -> int { return s.面积() }   // 动态分发（vtable）
    f(圆 { ... })            // struct 实参自动装箱为 dyn Trait

### 解构与默认值 / Destructuring & defaults

    match o {
        Some(Some(v)) => { ... }        // 嵌套解构
        Ok(Some(v))   => { ... }
        n if n > 0    => { ... }        // 裸标识符绑定 + 守卫
        _ => { ... }
    }

    v := o or 0                         // Option 默认值（Some(v)->v, None->0）

---

## 6. 结构体 / 枚举 / trait

    struct 点 { x: int, y: int }
    p := 点 { x: 1, y: 2 }

    enum 形状 { Circle(f64) Rect(f64, f64) Unit }
    match s { 形状::Circle(r) => { ... } _ => { ... } }

    trait 形状 { fn 面积(self) -> f64 }
    impl 形状 for 圆 { fn 面积(self) -> f64 { ... } }
    fn f(s: dyn 形状) { s.面积() }           // 动态分发

    @derive(Eq, Clone, Debug, Default, Hash, Ord, PartialEq, Display)
    struct 点 { x: int }

@derive 生成的方法：

| 派生 | 生成 | 说明 |
|---|---|---|
| Eq | eq / ne | 逐字段相等 |
| PartialEq | eq | 仅 eq |
| Clone | clone | 逐字段复制 |
| Debug | to_str | 类型名 { f: v } |
| Display | to_str | v1, v2 |
| Default | default | 字段零值 |
| Hash | hash | FNV 混合 |
| Ord | cmp/lt/le/gt/ge | 字典序 + 比较运算符重载 |

---

## 7. 运算符重载

    impl 向量 {
        fn add(self, o: 向量) -> 向量 { ... }
        fn eq(self, o: 向量) -> bool { ... }
        fn lt(self, o: 向量) -> bool { ... }
    }
    a + b   // → 向量__add(a, b)
    a == b  // → 向量__eq(a, b)

可重载：add sub mul div rem eq ne lt le gt ge neg

---

## 8. 并发 / Concurrency

    go 工作者(42)          // 新线程调用（fire-and-forget）
    ch := chan()           // 无界通道
    chan_send(ch, v)
    v := chan_recv(ch)     // 阻塞接收
    sleep(500)             // 毫秒

双后端：JIT 用 std::thread + Condvar；编译用 CreateThread/pthread。

---

## 9. C 交互 / C Interop

    C {                                    // 内联 C 块
        static long long 平方(long long x) { return x * x; }
        static void 回调(const char *s) { gt_报告(s); }
    }
    put(平方(5))                            // 自动解析签名

    extern "C" { fn puts(s: str) -> int }  // 直接声明 C 函数

    // C 头：只适用于"项目内可读的 .h"（会解析其函数签名，可用 别名.函数 调用）
    import c "native.h" as n
    put(n.原生函数(2.0))

    // 系统头（如 math.h）：改用内联 C 块（由 C 编译器 #include）
    C {
        #include <math.h>
        static double 平方根(double x) { return sqrt(x); }
    }
    put(平方根(2.0))

---

## 10. 错误处理 / Errors

    fn f(n: int) -> Result[int, str] {
        if n < 0 { return Err("负数") }
        return Ok(n * 2)
    }
    v := f(21)?              // ? 传播
    match f(-1) { Ok(v) => { ... } Err(e) => { ... } }
    o := Some(1)             // Option
    v := o or 0              // 默认值：Some(v) 取 v，None 取 0
    try { throw "异常" } expt e { put(e) } fily { ... }

---

## 11. 数据扩展

    t := (1, 2)             // 元组
    let (a, b) = t          // 解构
    s[0..3]                 // 切片
    l[-1]                   // 负索引
    a, b = b, a             // 解包交换
    [x * 2 for x in l if x > 0]  // 列表推导

---

## 12. 模块 / Modules

    import math             // 内置库（无引号）
    import math.vector      // 用户模块 a/b/c.gt
    import "x.gt" as x      // 文件（引号）
    import c "a.h"          // C 头（引号）

---

## 13. 宏 / Macros

    macro 平方(x) { (x) * (x) }
    put(平方(3))            // → (3) * (3)

---

## 14. 内置函数 / Builtins

| 类别 | 函数 |
|---|---|
| 输出 | put / print |
| 转换 | str / int / f64 / bool |
| 长度 | len |
| 数学 | abs min max sum range |
| 断言 | assert |
| 格式化 | pad_left / pad_right / fmt_int |
| 字符串 | upper lower trim split join find substr replace repeat |
| 容器 | push pop insert remove has keys values |
| 并发 | chan chan_send chan_recv sleep |
| 内存 | mem_alloc mem_free mem_store_i64 mem_load_i64 |
| 三元 | a if c else b |

---

## 14b. 标准库模块 / Stdlib modules

标准库以 `*.dll` 形式随 `res/lib` 分发，函数名与 **Python** 对齐。

### os
| 函数 | 签名 | 说明 |
|---|---|---|
| `getcwd()` | `() -> str` | 当前工作目录 |
| `getenv(name)` | `(str) -> str` | 环境变量（无则空串）|
| `setenv(k, v)` | `(str, str) -> bool` | 设置环境变量 |
| `path_exists(p)` | `(str) -> bool` | 路径是否存在 |
| `is_file(p)` / `is_dir(p)` | `(str) -> bool` | 文件 / 目录 |
| `getsize(p)` | `(str) -> int` | 字节数（失败 -1）|
| `listdir(p)` | `(str) -> str` | 目录项（`\n` 分隔）|
| `basename(p)` / `dirname(p)` | `(str) -> str` | 文件名 / 父目录 |
| `path_join(a, b)` | `(str, str) -> str` | 拼接路径 |
| `abspath(p)` | `(str) -> str` | 绝对路径 |
| `mkdir(p)` / `rmdir(p)` | `(str) -> bool` | 创建 / 删除目录 |
| `os_remove(p)` | `(str) -> bool` | 删除文件 |
| `system(cmd)` | `(str) -> int` | 执行命令，返回退出码 |

### json
| 函数 | 签名 | 说明 |
|---|---|---|
| `json_dumps(s)` | `(str) -> str` | 转义为 JSON 字符串 |
| `json_loads(s)` | `(str) -> str` | 反转义 |
| `json_dump(s, path)` | `(str, str) -> bool` | 写入文件 |
| `json_load(path)` | `(str) -> str` | 读文件 |

### toml
| 函数 | 签名 | 说明 |
|---|---|---|
| `toml_loads(text)` | `(str) -> str` | 解析为 `k=v;k=v` |
| `toml_load(path)` | `(str) -> str` | 读文件 |

### math / string
**Python 风格**：`sqrt` / `pow` / `floor` / `sin` …；`capitalize` / `title` / `zfill` / `isalpha` …

## 15. 所有权 / Ownership

- move：非 Copy 值传递即转移
- 借用：&T（共享）/ &mut T（独占）
- 流敏感 NLL：借用止于最后一次使用；分支 join；return/break 后不可达

---

## 16. 内存安全

- 数组/列表边界检查
- 除零检查
- 加/减/乘溢出检测（双后端）

---

## 17. 诊断 / Diagnostics

- 稳定错误码：E001（词法）～ E8xx（所有权）
- **智能建议**（`Help` 行）：未定义**变量/函数**时提示"是否想用 X？"（Levenshtein ≤ 2，**中英双语**）
- 修复建议：Help 行
- ariadne 定位：源码片段 + 精确列
- 双语：末尾加 zh 切换中文
- 多错误：语法阶段 panic-mode 恢复，一次报全部
