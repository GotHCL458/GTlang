# GTLang vs Python —— 语法与功能对比

> 基于 gtc_rust 当前实现对比 GTLang 与 Python 3.12。

## 1. 总览

| 维度 | GTLang | Python 3.12 |
|---|---|---|
| 类型系统 | **静态**（推导 + 可选标注） | 动态 |
| 执行 | 编译 LLVM 到 exe / Cranelift JIT | 解释 CPython |
| 缩进敏感 | 否（大括号分块） | 是 |
| 标识符 | 中文与任意 Unicode | Unicode |
| 内存模型 | 值（struct/数组）+ 引用（容器） | 全引用 |
| 泛型 | 单态化 | TypeVar 标注 |
| 所有权/借用 | **NLL 借用检查** | 无（GC） |
| 错误处理 | Result + `?` 传播 | 异常 |
| 并发 | **真线程**（go + chan） | GIL 限制 |

## 2. 变量与常量

| 特性 | GTLang | Python |
|---|---|---|
| 声明 | `x := 1` / `a = 1` | `x = 1` |
| 常量 | `const K = 10` | （约定） |
| 类型标注 | `let n: int = 4` | `n: int = 4` |
| 解构 | `let (a, b) = t` | `a, b = t` |

## 3. 基本类型

| 类型 | GTLang | Python |
|---|---|---|
| 整数 | int（i64，溢出检查） | int（任意精度） |
| 浮点 | f64 | float |
| 布尔 | bool | bool |
| 字符串 | str（UTF-8） | str（Unicode） |
| 定长数组 | [T; N]（值语义） | 无 |
| 列表 | list（引用，`l[i]`） | list |
| 集合 | set | set |
| 映射 | map | dict |
| 元组 | (T1, T2) | tuple |
| 枚举 | enum（带载荷） | Enum |
| 可选/结果 | Option / Result | Optional |

## 4. 语法对比

| 场景 | GTLang | Python |
|---|---|---|
| 函数 | `fn f(x: int) -> int { x + 1 }` | `def f(x: int) -> int: return x + 1` |
| 打印 | `put(x)` | `print(x)` |
| 插值 | `"值=$x"` / `"${x}"` | f-string `f"值={x}"` |
| 范围循环 | `for i in 0..n` | `for i in range(n)` |
| 三元 | `a if c else b` | `a if c else b` |
| 匹配 | `match v { 1 => … _ => … }` | `match`（3.10+） |
| 闭包 | `|x| x * 2` | `lambda x: x * 2` |
| 类 | `struct` + `impl` | `class` |
| trait | `trait T` + `impl T for X` | 鸭子类型 |

## 5. 关键差异

- **类型**：GTLang 编译期检查；Python 运行时
- **性能**：GTLang 编译机器码（loop 追平 C）；Python 字节码解释（~1000x 慢）
- **并发**：GTLang 真线程 + 通道；Python GIL
- **内存**：GTLang struct 栈分配；Python 全堆 + GC

## 6. 迁移示例

Python:
```python
def fib(n):
    if n < 2: return n
    return fib(n-1) + fib(n-2)
print([fib(i) for i in range(10)])
```

GTLang:
```gt
fn fib(n: int) -> int {
    if n < 2 { return n }
    return fib(n - 1) + fib(n - 2)
}
fn main() {
    for i in 0..10 { put(fib(i)) }
}
```
