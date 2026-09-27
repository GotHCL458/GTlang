# GTLang vs Python / 对比

> [English](en/COMPARISON.md)

> 面向熟悉 Python 的用户，快速了解差异。

---

## 1. 总览

| 维度 | GTLang | Python |
|---|---|---|
| 类型 | **静态**（编译期检查） | 动态 |
| 执行 | **编译/ JIT**（LLVM/Cranelift） | 解释（CPython） |
| 中文标识符 | **原生支持** | 支持（PEP 3131） |
| 缩进 | **花括号** | 缩进 |
| 并发 | **真线程**（go + chan） | GIL 限制 |
| 错误处理 | Result/Option + try/expt | try/except |
| 内存 | 手动/RC | GC |
| 性能 | 编译为机器码 | 字节码 |

---

## 2. 语法对比

| 场景 | GTLang | Python |
|---|---|---|
| 变量 | `x := 1` / `a = 1` | `x = 1` |
| 常量 | `const K = 5` | （约定大写） |
| 类型标注 | `let n: int = 4` | `n: int = 4`（可选） |
| 函数 | `fn f(x: int) -> int { x + 1 }` | `def f(x: int) -> int: return x + 1` |
| 打印 | `put(x)` | `print(x)` |
| 字符串插值 | `"值=$x"` / `"${x}"` | f-string `f"值={x}"` |
| 列表 | `list()` + `push` | `[]` + `append` |
| 字典 | `map()` + `insert`/`get` | `{}` |
| 元组 | `(1, 2)`，`t.0` | `(1, 2)`，`t[0]` |
| 范围循环 | `for i in 0..n` | `for i in range(n)` |
| 迭代 | `for v in l` | `for v in l` |
| 三元 | `a if c else b` | `a if c else b` |
| 匹配 | `match v { 1 => … _ => … }` | `match`（3.10+） |
| 枚举 | `enum E { A B }` | `enum.Enum` |
| 类 | `struct` + `impl` | `class` |
| trait | `trait T` + `impl T for X` | 鸭子类型 / ABC |
| 闭包 | `|x| x * 2` | `lambda x: x * 2` |
| 错误 | `Result[T,E]` + `?` | 异常 |

---

## 3. 关键差异

### 3.1 类型
- **GTLang**：静态检查，类型错误编译期报出
- **Python**：运行时才发现

### 3.2 性能
- **GTLang**：编译为机器码（LLVM），无解释开销
- **Python**：字节码解释，每步有开销

### 3.3 并发
- **GTLang**：`go f()` 真线程（无 GIL）+ `chan` 通道
- **Python**：`threading` 受 GIL 限制；`asyncio` 单线程

### 3.4 内存
- **GTLang**：结构体栈分配（零开销），容器引用语义
- **Python**：一切皆对象（堆），GC

### 3.5 元编程
- **GTLang**：声明式宏 + `@derive`
- **Python**：装饰器 + 元类（更强，运行时）

---

## 4. 迁移示例

### Python
```python
def fib(n):
    if n < 2: return n
    return fib(n-1) + fib(n-2)

print([fib(i) for i in range(10)])
```

### GTLang
```gt
fn fib(n: int) -> int {
    if n < 2 { return n }
    return fib(n - 1) + fib(n - 2)
}

fn main() {
    for i in 0..10 {
        put(fib(i))
    }
}
```

---

## 5. 各自优势

| GTLang 优势 | Python 优势 |
|---|---|
| 编译期类型安全 | 生态庞大（PyPI） |
| 原生性能 | 开发速度 |
| 无 GIL 并发 | 动态灵活性 |
| 中文标识符原生 | 库支持 |
| 双后端（开发/发布） | 成熟工具链 |
