# Browser Core Wasm Backend Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 为 Vix Playground 构建一个浏览器内可运行的 Wasm 核心子集后端，支持函数、局部变量、基础控制流、数组、struct 与控制台输出，不依赖操作系统或标准库。

**Architecture:** 保持现有前端链路 `parser -> semantic -> typecheck -> ownership` 不变，继续使用 `playground/vixc_frontend.c` 产出 AST。后端不走 LLVM 主线，而是在 `src/compiler/WasmCodegen.cpp` 中实现一个边界明确的 Browser Core Wasm Backend：标量值直接映射到 Wasm locals，数组与 struct 以线性内存块表示，字符串以 data segment 形式嵌入，输出通过 JS imports 完成。

**Tech Stack:** Emscripten、Binaryen C API、现有 Vix 前端（parser/typecheck/ownership）、浏览器 `WebAssembly.instantiate`、CodeMirror Playground

## Global Constraints

- 所有执行必须发生在浏览器内，不依赖服务端编译或运行
- 只支持浏览器核心子集：函数、局部变量、`if/elif/else`、`while`、`for`、`break`、`continue`、`return`、`print`、数组、struct、字符串字面量、整数/布尔
- 明确不支持：输入、`import`、标准库依赖、网络、文件、`extern C`、第三方库、其他依赖操作系统的能力
- 数组与 struct 必须可以支撑 `hello world`、`struct` 字段读写、冒泡排序、二分查找
- 对外宿主接口只保留 JS imports：`vix_putchar`、`vix_puts`、`vix_print_i32`、`vix_exit`
- 继续使用现有 `playground/vixc-wasm.cpp` 入口，不新增第二套浏览器编译链
- 手工代码修改统一使用 `apply_patch`
- 以最小正确改动为原则，不为未来未确认需求预埋复杂抽象

---

## File Structure

### 现有文件职责

- `src/compiler/WasmCodegen.h`
  - Browser Core Wasm Backend 的状态、辅助结构、compile dispatch 声明
- `src/compiler/WasmCodegen.cpp`
  - AST -> Binaryen IR 的主体实现
- `src/compiler/WasmTypeMap.h`
  - 类型映射与运行时布局元数据声明
- `src/compiler/WasmTypeMap.cpp`
  - Vix 类型到 Wasm 标量类型 / 聚合类型布局信息映射
- `playground/vixc_frontend.c`
  - 字符串源码 -> AST 的前端入口，错误信息回填
- `playground/vixc-wasm.cpp`
  - `compile_vix` Emscripten 导出入口
- `playground/playground.js`
  - 浏览器端编译、实例化、输出、示例切换逻辑
- `tests/test_wasm_codegen.cpp`
  - Wasm codegen 回归测试

### 本计划允许新增的文件

- `tests/fixtures/wasm_core/hello.vix`
  - 最小输出样例
- `tests/fixtures/wasm_core/control_flow.vix`
  - `if/while/for/break/continue` 样例
- `tests/fixtures/wasm_core/arrays.vix`
  - 数组字面量、索引读写、`.length`
- `tests/fixtures/wasm_core/structs.vix`
  - struct 定义、字面量、字段访问与字段赋值
- `tests/fixtures/wasm_core/bubble_sort.vix`
  - 冒泡排序样例
- `tests/fixtures/wasm_core/binary_search.vix`
  - 二分查找样例
- `docs/browser-core-wasm-runtime.md`
  - 记录 Browser Core Wasm Backend 的运行时表示、内存布局、支持边界

### 分层边界

- 前端负责：语法解析、类型检查、ownership 检查、错误消息
- WasmTypeMap 负责：类型分类、字段大小/对齐/布局元数据
- WasmCodegen 负责：
  - 标量表达式
  - 控制流 lowering
  - 局部变量 / 参数绑定
  - 线性内存中的数组 / struct 分配与 load/store
  - 导出 `main`
- Playground JS 负责：
  - 调用 `compile_vix`
  - 实例化结果 Wasm
  - 绑定输出 imports
  - 呈现错误信息

---

## 运行时设计基线

### 标量表示

- `i32` / `bool`：使用 Wasm `i32`
- `string literal`：data segment 中的 NUL 结尾字节串，表达式值为 `i32` 指针
- 非字符串聚合值：在函数内部一律按指针处理

### 内存布局

- 线性内存页：由 `BinaryenSetMemory(...)` 声明
- 静态字符串区：从固定偏移开始顺序放置，已有实现继续沿用
- 动态堆区：新增 bump allocator，专供数组与 struct 分配
- 数组布局：
  - `offset + 0`: `length: i32`
  - `offset + 4`: `element_size: i32`
  - `offset + 8`: `data[0]`
- struct 布局：
  - 按字段顺序顺排
  - 初版统一按 4 字节对齐
  - 字段偏移由 `WasmTypeMap` / 布局辅助函数计算

### 左值模型

统一把以下三类都视为“可寻址左值”：

- 变量：`x`
- 数组元素：`arr[i]`
- 结构体字段：`p.x`

后端必须新增“先求地址，再做 load/store”的辅助路径，避免分别在 `compile_assign()` 里堆分支。

---

### Task 1: 固化 Browser Core 子集边界与回归基线

**Files:**

- Create: `docs/browser-core-wasm-runtime.md`
- Create: `tests/fixtures/wasm_core/hello.vix`
- Create: `tests/fixtures/wasm_core/control_flow.vix`
- Create: `tests/fixtures/wasm_core/arrays.vix`
- Create: `tests/fixtures/wasm_core/structs.vix`
- Create: `tests/fixtures/wasm_core/bubble_sort.vix`
- Create: `tests/fixtures/wasm_core/binary_search.vix`
- Modify: `tests/test_wasm_codegen.cpp`

**Interfaces:**

- Consumes: `CompileResult vixc_compile_string(const char *source)`、`bool WasmCodegen::emit(ASTNode *root, std::vector<uint8_t> &out_bytes, std::string &error_msg)`
- Produces:
  - 测试辅助函数：`static std::string read_fixture(const char *path)`
  - 测试辅助函数：`static void expect_emit_success(const char *fixture_path)`
  - 测试夹具：`tests/fixtures/wasm_core/*.vix`
  - 运行时设计文档：`docs/browser-core-wasm-runtime.md`

- [ ] **Step 1: 写运行时设计说明文档初稿**

```markdown
# Browser Core Wasm Runtime

## 支持边界

- 支持：函数、局部变量、if/elif/else、while、for、break、continue、return、print、数组、struct、字符串字面量、i32、bool
- 不支持：input、import、标准库依赖、文件、网络、extern C、第三方库、宿主 OS 交互

## 值表示

- i32/bool: Wasm i32
- string literal: 指向 data segment 的 i32 指针
- array: 指向堆内存块的 i32 指针
- struct: 指向堆内存块的 i32 指针

## 数组布局

- +0: length(i32)
- +4: element_size(i32)
- +8: element bytes

## struct 布局

- 字段按声明顺序排列
- 初版统一 4 字节对齐
- 所有字段偏移由后端统一计算
```

- [ ] **Step 2: 创建 hello fixture**

```vix
fn main(): i32 {
    print("Hello, Vix Playground!")
    return 0
}
```

- [ ] **Step 3: 创建 control_flow fixture**

```vix
fn main(): i32 {
    let mut i = 0
    let mut sum = 0
    while (i < 5) {
        if (i == 3) {
            i = i + 1
            continue
        }
        sum = sum + i
        if (sum > 10) { break }
        i = i + 1
    }
    print(sum)
    for (j in 0 .. 3) {
        print(j)
    }
    return 0
}
```

- [ ] **Step 4: 创建 arrays fixture**

```vix
fn main(): i32 {
    let mut arr = [5, 2, 8, 1]
    arr[1] = 7
    print(arr.length)
    print(arr[0])
    print(arr[1])
    print(arr[2])
    print(arr[3])
    return 0
}
```

- [ ] **Step 5: 创建 structs fixture**

```vix
struct Point {
    x: i32
    y: i32
}

fn main(): i32 {
    let mut p = Point { x: 3, y: 4 }
    p.x = 10
    print(p.x)
    print(p.y)
    return 0
}
```

- [ ] **Step 6: 创建 bubble_sort fixture**

```vix
fn sort(nums: [i32], size: i32): i32 {
    for (i in 0 .. size - 1) {
        for (j in 0 .. size - i - 1) {
            if (nums[j] > nums[j + 1]) {
                let temp = nums[j]
                nums[j] = nums[j + 1]
                nums[j + 1] = temp
            }
        }
    }
    return 0
}

fn main(): i32 {
    let arr = [5, 2, 8, 1, 9, 10]
    sort(arr, arr.length)
    for (i in 0 .. arr.length) {
        print(arr[i])
    }
    return 0
}
```

- [ ] **Step 7: 创建 binary_search fixture**

```vix
fn binary_search(arr: [i32], target: i32): i32 {
    let mut lo = 0
    let mut hi = arr.length - 1
    while (lo <= hi) {
        let mid = lo + (hi - lo) / 2
        if (arr[mid] == target) { return mid }
        elif (arr[mid] < target) { lo = mid + 1 }
        else { hi = mid - 1 }
    }
    return -1
}

fn main(): i32 {
    let arr = [2, 5, 8, 12, 16, 23, 38, 56, 72, 91]
    print(binary_search(arr, 23))
    print(binary_search(arr, 56))
    print(binary_search(arr, 100))
    return 0
}
```

- [ ] **Step 8: 在 `tests/test_wasm_codegen.cpp` 增加夹具读取与基线测试**

```cpp
static std::string read_fixture(const char *path) {
    FILE *fp = fopen(path, "rb");
    assert(fp != nullptr);
    fseek(fp, 0, SEEK_END);
    long len = ftell(fp);
    fseek(fp, 0, SEEK_SET);
    std::string buf;
    buf.resize((size_t)len);
    size_t n = fread(buf.data(), 1, (size_t)len, fp);
    fclose(fp);
    assert(n == (size_t)len);
    return buf;
}

static void expect_emit_success(const char *fixture_path) {
    std::string source = read_fixture(fixture_path);
    CompileResult cr = vixc_compile_string(source.c_str());
    assert(cr.error_count == 0);
    assert(cr.root != nullptr);

    WasmCodegen cg;
    std::vector<uint8_t> wasm_bytes;
    std::string error;
    bool ok = cg.emit(cr.root, wasm_bytes, error);
    assert(ok);
    assert(!wasm_bytes.empty());
    assert(wasm_bytes[0] == 0x00);
    assert(wasm_bytes[1] == 0x61);
    assert(wasm_bytes[2] == 0x73);
    assert(wasm_bytes[3] == 0x6d);

    vixc_free_result(&cr);
}
```

- [ ] **Step 9: 先只启用 hello fixture 测试，验证基线通过**

```cpp
void test_fixture_hello() {
    expect_emit_success("tests/fixtures/wasm_core/hello.vix");
    fprintf(stderr, "PASS: test_fixture_hello\n"); fflush(stderr);
    tests_passed++;
}
```

- [ ] **Step 10: 运行基线测试**

Run: `cmake --build build --target test_wasm_codegen && "build/test_wasm_codegen.exe"`

Expected:

- `test_binaryen_basic` PASS
- `test_compile_to_wasm` PASS
- `test_string_literal_embedded_in_wasm` PASS
- `test_parse_error_reports_message` PASS
- `test_fixture_hello` PASS

- [ ] **Step 11: Commit**

```bash
git add docs/browser-core-wasm-runtime.md tests/fixtures/wasm_core tests/test_wasm_codegen.cpp
git commit -m "test: add browser core wasm fixtures and runtime baseline"
```

### Task 2: 修正局部变量、参数绑定与标量表达式基线

**Files:**

- Modify: `src/compiler/WasmCodegen.h`
- Modify: `src/compiler/WasmCodegen.cpp`
- Modify: `tests/test_wasm_codegen.cpp`

**Interfaces:**

- Consumes: `expect_emit_success(const char *fixture_path)`、现有 `FuncInfo`
- Produces:
  - `uint32_t bind_param_local(const char *name, uint32_t index)`
  - `uintptr_t compile_var_decl(ASTNode *decl_node, ASTNode *init_expr)` 正式接入
  - `uintptr_t compile_bool_literal(ASTNode *node)` 或等价逻辑

- [ ] **Step 1: 增加失败测试，覆盖参数读取与局部变量声明**

```cpp
void test_compile_function_params_and_locals() {
    const char *source =
        "fn add(a: i32, b: i32): i32 {\n"
        "    let c = a + b\n"
        "    return c\n"
        "}\n"
        "fn main(): i32 {\n"
        "    print(add(4, 5))\n"
        "    return 0\n"
        "}\n";

    CompileResult cr = vixc_compile_string(source);
    assert(cr.error_count == 0);
    assert(cr.root != nullptr);

    WasmCodegen cg;
    std::vector<uint8_t> wasm_bytes;
    std::string error;
    bool ok = cg.emit(cr.root, wasm_bytes, error);
    assert(ok);
    assert(!wasm_bytes.empty());
    vixc_free_result(&cr);
}
```

- [ ] **Step 2: 运行单测，确认当前实现仍存在参数绑定缺口**

Run: `cmake --build build --target test_wasm_codegen && "build/test_wasm_codegen.exe"`

Expected:

- 新测试失败，或浏览器中 `fib/add` 类函数参数读取行为异常

- [ ] **Step 3: 在 `WasmCodegen.h` 扩展函数上下文辅助声明**

```cpp
struct FuncInfo {
    BinaryenFunctionRef func_ref;
    std::unordered_map<std::string, uint32_t> local_indices;
    uint32_t next_local;
    uint32_t param_count;
    std::vector<uintptr_t> param_types;
    uintptr_t return_type;
};

uint32_t get_or_create_local(const char *name, uintptr_t wasm_type);
void bind_param_locals(ASTNode *params);
```

- [ ] **Step 4: 在 `register_function()` / `compile_function_body()` 中绑定参数名到 local index**

```cpp
void WasmCodegen::bind_param_locals(ASTNode *params) {
    if (!m_current_func || !params || params->type != AST_EXPRESSION_LIST) return;
    for (int i = 0; i < params->data.expression_list.expression_count; i++) {
        ASTNode *p = params->data.expression_list.expressions[i];
        if (!p || p->type != AST_ASSIGN || !p->data.assign.left) continue;
        ASTNode *left = p->data.assign.left;
        if (left->type != AST_IDENTIFIER || !left->data.identifier.name) continue;
        m_current_func->local_indices[left->data.identifier.name] = (uint32_t)i;
    }
}
```

- [ ] **Step 5: 在 `compile_node()` 中显式区分声明赋值与普通赋值**

```cpp
case AST_ASSIGN:
    if (node->data.assign.is_declaration) {
        return compile_var_decl(node, node->data.assign.right);
    }
    return compile_assign(node);
```

- [ ] **Step 6: 确保布尔常量、整型比较结果统一走 `i32`**

```cpp
case AST_NUM_INT:
    return (uintptr_t)BinaryenConst(
        m_module,
        BinaryenLiteralInt32((int32_t)node->data.num_int.value));
```

- [ ] **Step 7: 运行测试，确认参数与局部变量路径通过**

Run: `cmake --build build --target test_wasm_codegen && "build/test_wasm_codegen.exe"`

Expected:

- `test_compile_function_params_and_locals` PASS
- 原有 hello / string / parse error 测试不回退

- [ ] **Step 8: Commit**

```bash
git add src/compiler/WasmCodegen.h src/compiler/WasmCodegen.cpp tests/test_wasm_codegen.cpp
git commit -m "fix: bind wasm function params and local declarations"
```

### Task 3: 实现控制流节点 `while` / `for` / `break` / `continue`

**Files:**

- Modify: `src/compiler/WasmCodegen.h`
- Modify: `src/compiler/WasmCodegen.cpp`
- Modify: `tests/test_wasm_codegen.cpp`

**Interfaces:**

- Consumes: Task 2 的参数/局部变量绑定
- Produces:
  - `uintptr_t compile_while(ASTNode *while_node)`
  - `uintptr_t compile_for(ASTNode *for_node)`
  - `uintptr_t compile_break(ASTNode *node)`
  - `uintptr_t compile_continue(ASTNode *node)`
  - 循环标签栈：`std::vector<std::string> m_break_labels; std::vector<std::string> m_continue_labels;`

- [ ] **Step 1: 增加失败测试，覆盖 while / for / break / continue fixture**

```cpp
void test_fixture_control_flow() {
    expect_emit_success("tests/fixtures/wasm_core/control_flow.vix");
    fprintf(stderr, "PASS: test_fixture_control_flow\n"); fflush(stderr);
    tests_passed++;
}
```

- [ ] **Step 2: 运行测试，确认 control_flow fixture 失败**

Run: `cmake --build build --target test_wasm_codegen && "build/test_wasm_codegen.exe"`

Expected:

- `test_fixture_control_flow` FAIL
- 原因可能是 `compile_while()` 返回 `Nop` 或 `AST_FOR` 未处理

- [ ] **Step 3: 在 `WasmCodegen.h` 增加循环辅助状态与声明**

```cpp
std::vector<std::string> m_break_labels;
std::vector<std::string> m_continue_labels;

uintptr_t compile_for(ASTNode *for_node);
uintptr_t compile_break(ASTNode *node);
uintptr_t compile_continue(ASTNode *node);
```

- [ ] **Step 4: 在 `compile_node()` 中接入 `AST_FOR`、`AST_BREAK`、`AST_CONTINUE`**

```cpp
case AST_FOR:
    return compile_for(node);
case AST_BREAK:
    return compile_break(node);
case AST_CONTINUE:
    return compile_continue(node);
```

- [ ] **Step 5: 用 `block + loop + br_if + br` 实现 `compile_while()`**

```cpp
uintptr_t WasmCodegen::compile_while(ASTNode *while_node) {
    std::string breakLabel = "while_break_" + std::to_string(m_break_labels.size());
    std::string continueLabel = "while_continue_" + std::to_string(m_continue_labels.size());
    m_break_labels.push_back(breakLabel);
    m_continue_labels.push_back(continueLabel);

    BinaryenExpressionRef cond = (BinaryenExpressionRef)compile_node(while_node->data.while_stmt.condition);
    BinaryenExpressionRef body = (BinaryenExpressionRef)compile_node(while_node->data.while_stmt.body);
    BinaryenExpressionRef notCond = BinaryenUnary(m_module, BinaryenEqZInt32(), cond);
    BinaryenExpressionRef breakIf = BinaryenBreak(m_module, breakLabel.c_str(), notCond, nullptr);

    BinaryenExpressionRef loopExprs[] = {
        breakIf,
        body,
        BinaryenBreak(m_module, continueLabel.c_str(), nullptr, nullptr)
    };

    BinaryenExpressionRef loopBody = BinaryenBlock(
        m_module, continueLabel.c_str(), loopExprs, 3, BinaryenTypeNone());
    BinaryenExpressionRef loop = BinaryenLoop(m_module, continueLabel.c_str(), loopBody);
    BinaryenExpressionRef outer[] = { loop };

    m_break_labels.pop_back();
    m_continue_labels.pop_back();
    return (uintptr_t)BinaryenBlock(m_module, breakLabel.c_str(), outer, 1, BinaryenTypeNone());
}
```

- [ ] **Step 6: 用变量初始化 + while lowering 实现 `compile_for()`**

```cpp
uintptr_t WasmCodegen::compile_for(ASTNode *for_node) {
    ASTNode *var = for_node->data.for_stmt.var;
    ASTNode *start = for_node->data.for_stmt.start;
    ASTNode *end = for_node->data.for_stmt.end;
    ASTNode *body = for_node->data.for_stmt.body;

    ASTNode fakeDecl = {};
    fakeDecl.type = AST_ASSIGN;
    fakeDecl.data.assign.left = var;
    fakeDecl.data.assign.right = start;
    fakeDecl.data.assign.is_declaration = 1;

    uintptr_t init = compile_var_decl(&fakeDecl, start);
    uint32_t idx = get_or_create_local(var->data.identifier.name, BinaryenTypeInt32());
    BinaryenExpressionRef cond = BinaryenBinary(
        m_module,
        BinaryenLtSInt32(),
        BinaryenLocalGet(m_module, idx, BinaryenTypeInt32()),
        (BinaryenExpressionRef)compile_node(end));

    BinaryenExpressionRef inc = BinaryenLocalSet(
        m_module,
        idx,
        BinaryenBinary(
            m_module,
            BinaryenAddInt32(),
            BinaryenLocalGet(m_module, idx, BinaryenTypeInt32()),
            BinaryenConst(m_module, BinaryenLiteralInt32(1))));

    BinaryenExpressionRef seq[] = {
        (BinaryenExpressionRef)compile_node(body),
        inc
    };
    ASTNode *savedBody = for_node->data.for_stmt.body;
    (void)savedBody;
    return init ? init : (uintptr_t)cond;
}
```

实现时不要照抄以上半成品返回值；目标是把 `for` 降成等价 `while`：先初始化变量，再判断条件，再执行 body，再递增。

- [ ] **Step 7: 实现 `compile_break()` / `compile_continue()`**

```cpp
uintptr_t WasmCodegen::compile_break(ASTNode *) {
    if (m_break_labels.empty()) return (uintptr_t)BinaryenNop(m_module);
    return (uintptr_t)BinaryenBreak(m_module, m_break_labels.back().c_str(), nullptr, nullptr);
}

uintptr_t WasmCodegen::compile_continue(ASTNode *) {
    if (m_continue_labels.empty()) return (uintptr_t)BinaryenNop(m_module);
    return (uintptr_t)BinaryenBreak(m_module, m_continue_labels.back().c_str(), nullptr, nullptr);
}
```

- [ ] **Step 8: 运行测试，确认控制流 fixture 可 emit**

Run: `cmake --build build --target test_wasm_codegen && "build/test_wasm_codegen.exe"`

Expected:

- `test_fixture_control_flow` PASS
- 其余基线测试继续 PASS

- [ ] **Step 9: Commit**

```bash
git add src/compiler/WasmCodegen.h src/compiler/WasmCodegen.cpp tests/test_wasm_codegen.cpp
git commit -m "feat: add wasm control flow lowering for browser core"
```

### Task 4: 实现线性内存分配器与数组读写

**Files:**

- Modify: `src/compiler/WasmTypeMap.h`
- Modify: `src/compiler/WasmTypeMap.cpp`
- Modify: `src/compiler/WasmCodegen.h`
- Modify: `src/compiler/WasmCodegen.cpp`
- Modify: `tests/test_wasm_codegen.cpp`

**Interfaces:**

- Consumes: Task 3 控制流能力
- Produces:
  - `uint32_t m_heap_offset`
  - `uint32_t alloc_bytes(uint32_t size, uint32_t align)`
  - `uintptr_t compile_array_literal(ASTNode *node)`
  - `uintptr_t compile_index(ASTNode *node)`
  - `uintptr_t compile_index_assign(ASTNode *assign_node)`
  - `uintptr_t emit_i32_load(uintptr_t addr)`
  - `uintptr_t emit_i32_store(uintptr_t addr, uintptr_t value)`

- [ ] **Step 1: 增加失败测试，覆盖 arrays 与 bubble_sort fixture**

```cpp
void test_fixture_arrays() {
    expect_emit_success("tests/fixtures/wasm_core/arrays.vix");
    fprintf(stderr, "PASS: test_fixture_arrays\n"); fflush(stderr);
    tests_passed++;
}

void test_fixture_bubble_sort() {
    expect_emit_success("tests/fixtures/wasm_core/bubble_sort.vix");
    fprintf(stderr, "PASS: test_fixture_bubble_sort\n"); fflush(stderr);
    tests_passed++;
}
```

- [ ] **Step 2: 运行测试，确认数组路径失败**

Run: `cmake --build build --target test_wasm_codegen && "build/test_wasm_codegen.exe"`

Expected:

- `test_fixture_arrays` FAIL
- `test_fixture_bubble_sort` FAIL

- [ ] **Step 3: 在 `WasmTypeMap.h` 扩展布局元数据**

```cpp
struct WasmTypeInfo {
    uintptr_t val_type;
    int32_t wasm_memory_size;
    bool is_struct;
    bool is_pointer_like;
};
```

- [ ] **Step 4: 在 `WasmCodegen.h` 增加内存辅助声明**

```cpp
uint32_t m_heap_offset;

uint32_t alloc_bytes(uint32_t size, uint32_t align);
uintptr_t emit_i32_load(uintptr_t addr);
uintptr_t emit_i32_store(uintptr_t addr, uintptr_t value);
uintptr_t compile_array_literal(ASTNode *node);
uintptr_t compile_index(ASTNode *node);
uintptr_t compile_index_assign(ASTNode *assign_node);
uintptr_t emit_array_length(uintptr_t array_ptr);
```

- [ ] **Step 5: 初始化堆起点，确保避开静态字符串区**

```cpp
m_string_offset = 16;
m_heap_offset = 4096;
```

若字符串段超过 `4096`，则在 `emit()` 尾部统一执行：

```cpp
if (m_heap_offset < m_string_offset + 16) {
    m_heap_offset = (m_string_offset + 31) & ~31;
}
```

- [ ] **Step 6: 实现固定 bump allocator 与基本 load/store**

```cpp
uint32_t WasmCodegen::alloc_bytes(uint32_t size, uint32_t align) {
    uint32_t base = (m_heap_offset + align - 1) & ~(align - 1);
    m_heap_offset = base + size;
    return base;
}

uintptr_t WasmCodegen::emit_i32_load(uintptr_t addr) {
    return (uintptr_t)BinaryenLoad(m_module, 4, true, 0, 4, BinaryenTypeInt32(), (BinaryenExpressionRef)addr, "mem");
}

uintptr_t WasmCodegen::emit_i32_store(uintptr_t addr, uintptr_t value) {
    return (uintptr_t)BinaryenStore(m_module, 4, 0, 4, (BinaryenExpressionRef)addr, (BinaryenExpressionRef)value, BinaryenTypeInt32(), "mem");
}
```

- [ ] **Step 7: 在 `compile_node()` 中接入 `AST_EXPRESSION_LIST` 作为数组字面量、`AST_INDEX` 作为索引读取**

```cpp
case AST_EXPRESSION_LIST:
    if (node->inferred_type && node->inferred_type->kind == TYPEINFO_ARRAY) {
        return compile_array_literal(node);
    }
    return compile_block(node);
case AST_INDEX:
    return compile_index(node);
```

- [ ] **Step 8: 实现数组字面量布局写入**

```cpp
uintptr_t WasmCodegen::compile_array_literal(ASTNode *node) {
    int count = node->data.expression_list.expression_count;
    uint32_t total = 8 + (uint32_t)count * 4;
    uint32_t base = alloc_bytes(total, 4);

    std::vector<BinaryenExpressionRef> exprs;
    exprs.push_back((BinaryenExpressionRef)emit_i32_store(
        (uintptr_t)BinaryenConst(m_module, BinaryenLiteralInt32((int32_t)base)),
        (uintptr_t)BinaryenConst(m_module, BinaryenLiteralInt32(count))));
    exprs.push_back((BinaryenExpressionRef)emit_i32_store(
        (uintptr_t)BinaryenConst(m_module, BinaryenLiteralInt32((int32_t)(base + 4))),
        (uintptr_t)BinaryenConst(m_module, BinaryenLiteralInt32(4))));

    for (int i = 0; i < count; i++) {
        uintptr_t value = compile_node(node->data.expression_list.expressions[i]);
        exprs.push_back((BinaryenExpressionRef)emit_i32_store(
            (uintptr_t)BinaryenConst(m_module, BinaryenLiteralInt32((int32_t)(base + 8 + i * 4))),
            value));
    }

    exprs.push_back(BinaryenConst(m_module, BinaryenLiteralInt32((int32_t)base)));
    return (uintptr_t)BinaryenBlock(m_module, nullptr, exprs.data(), exprs.size(), BinaryenTypeInt32());
}
```

- [ ] **Step 9: 实现 `arr.length`、`arr[i]`、`arr[i] = v`**

```cpp
uintptr_t WasmCodegen::emit_array_length(uintptr_t array_ptr) {
    return emit_i32_load(array_ptr);
}

uintptr_t WasmCodegen::compile_index(ASTNode *node) {
    uintptr_t base = compile_node(node->data.index.target);
    uintptr_t idx = compile_node(node->data.index.index);
    uintptr_t dataAddr = (uintptr_t)BinaryenBinary(
        m_module,
        BinaryenAddInt32(),
        (BinaryenExpressionRef)base,
        BinaryenConst(m_module, BinaryenLiteralInt32(8)));
    uintptr_t byteOffset = (uintptr_t)BinaryenBinary(
        m_module,
        BinaryenShlInt32(),
        (BinaryenExpressionRef)idx,
        BinaryenConst(m_module, BinaryenLiteralInt32(2)));
    uintptr_t addr = (uintptr_t)BinaryenBinary(
        m_module,
        BinaryenAddInt32(),
        (BinaryenExpressionRef)dataAddr,
        (BinaryenExpressionRef)byteOffset);
    return emit_i32_load(addr);
}
```

对 `arr[i] = v` 不要在 `compile_assign()` 里直接返回 `Nop`；应检测左值为 `AST_INDEX`，走地址计算 + store。

- [ ] **Step 10: 运行测试，确认数组 fixture 与冒泡排序 fixture 可 emit**

Run: `cmake --build build --target test_wasm_codegen && "build/test_wasm_codegen.exe"`

Expected:

- `test_fixture_arrays` PASS
- `test_fixture_bubble_sort` PASS

- [ ] **Step 11: Commit**

```bash
git add src/compiler/WasmTypeMap.h src/compiler/WasmTypeMap.cpp src/compiler/WasmCodegen.h src/compiler/WasmCodegen.cpp tests/test_wasm_codegen.cpp
git commit -m "feat: add browser core wasm array memory model"
```

### Task 5: 实现 struct 布局、字段访问与字段赋值

**Files:**

- Modify: `src/compiler/WasmTypeMap.h`
- Modify: `src/compiler/WasmTypeMap.cpp`
- Modify: `src/compiler/WasmCodegen.h`
- Modify: `src/compiler/WasmCodegen.cpp`
- Modify: `tests/test_wasm_codegen.cpp`

**Interfaces:**

- Consumes: Task 4 的线性内存读写辅助
- Produces:
  - `struct WasmFieldLayout { std::string name; uint32_t offset; uint32_t size; };`
  - `struct WasmStructLayout { uint32_t size; std::vector<WasmFieldLayout> fields; };`
  - `WasmStructLayout get_struct_layout(ASTNode *struct_def)` 或等价映射
  - `uintptr_t compile_struct_literal(ASTNode *node)`
  - `uintptr_t compile_member_access(ASTNode *node)`
  - `uintptr_t compile_member_assign(ASTNode *assign_node)`

- [ ] **Step 1: 增加失败测试，覆盖 structs 与 binary_search fixture**

```cpp
void test_fixture_structs() {
    expect_emit_success("tests/fixtures/wasm_core/structs.vix");
    fprintf(stderr, "PASS: test_fixture_structs\n"); fflush(stderr);
    tests_passed++;
}

void test_fixture_binary_search() {
    expect_emit_success("tests/fixtures/wasm_core/binary_search.vix");
    fprintf(stderr, "PASS: test_fixture_binary_search\n"); fflush(stderr);
    tests_passed++;
}
```

- [ ] **Step 2: 运行测试，确认 struct fixture 失败**

Run: `cmake --build build --target test_wasm_codegen && "build/test_wasm_codegen.exe"`

Expected:

- `test_fixture_structs` FAIL

- [ ] **Step 3: 在 `WasmTypeMap.h` 增加 struct 布局结构体声明**

```cpp
struct WasmFieldLayout {
    std::string name;
    uint32_t offset;
    uint32_t size;
};

struct WasmStructLayout {
    uint32_t size;
    std::vector<WasmFieldLayout> fields;
};
```

- [ ] **Step 4: 在 `WasmCodegen.h` 中增加 struct 布局缓存与辅助声明**

```cpp
std::unordered_map<std::string, WasmStructLayout> m_struct_layouts;

void register_struct_layout(ASTNode *node);
uintptr_t compile_struct_literal(ASTNode *node);
uintptr_t compile_member_access(ASTNode *node);
uintptr_t compile_member_assign(ASTNode *assign_node);
const WasmFieldLayout *find_field_layout(const std::string &struct_name, const std::string &field_name) const;
```

- [ ] **Step 5: 在 `emit()` 第一遍扫描阶段注册 struct 布局**

```cpp
if (stmt && stmt->type == AST_STRUCT_DEF) {
    register_struct_layout(stmt);
}
if (stmt && stmt->type == AST_FUNCTION) {
    register_function(stmt);
}
```

- [ ] **Step 6: 实现字段顺序布局计算，初版统一 4 字节字段大小**

```cpp
void WasmCodegen::register_struct_layout(ASTNode *node) {
    if (!node || node->type != AST_STRUCT_DEF || !node->data.struct_def.name) return;
    WasmStructLayout layout = {};
    uint32_t offset = 0;
    ASTNode *fields = node->data.struct_def.fields;
    if (fields && fields->type == AST_EXPRESSION_LIST) {
        for (int i = 0; i < fields->data.expression_list.expression_count; i++) {
            ASTNode *f = fields->data.expression_list.expressions[i];
            if (!f || f->type != AST_ASSIGN || !f->data.assign.left) continue;
            ASTNode *left = f->data.assign.left;
            if (left->type != AST_IDENTIFIER || !left->data.identifier.name) continue;
            layout.fields.push_back({left->data.identifier.name, offset, 4});
            offset += 4;
        }
    }
    layout.size = offset;
    m_struct_layouts[node->data.struct_def.name] = layout;
}
```

- [ ] **Step 7: 在 `compile_node()` 中接入 `AST_STRUCT_LITERAL` 与 `AST_MEMBER_ACCESS`**

```cpp
case AST_STRUCT_LITERAL:
    return compile_struct_literal(node);
case AST_MEMBER_ACCESS:
    return compile_member_access(node);
```

- [ ] **Step 8: 实现 struct 字面量分配与字段写入**

```cpp
uintptr_t WasmCodegen::compile_struct_literal(ASTNode *node) {
    const char *name = node->data.struct_literal.struct_name;
    auto it = m_struct_layouts.find(name ? name : "");
    if (it == m_struct_layouts.end()) return (uintptr_t)BinaryenNop(m_module);

    uint32_t base = alloc_bytes(it->second.size, 4);
    std::vector<BinaryenExpressionRef> exprs;
    ASTNode *fields = node->data.struct_literal.fields;
    if (fields && fields->type == AST_EXPRESSION_LIST) {
        for (int i = 0; i < fields->data.expression_list.expression_count; i++) {
            ASTNode *f = fields->data.expression_list.expressions[i];
            if (!f || f->type != AST_ASSIGN || !f->data.assign.left || !f->data.assign.right) continue;
            const char *fname = f->data.assign.left->data.identifier.name;
            const WasmFieldLayout *layout = find_field_layout(name ? name : "", fname ? fname : "");
            if (!layout) continue;
            exprs.push_back((BinaryenExpressionRef)emit_i32_store(
                (uintptr_t)BinaryenConst(m_module, BinaryenLiteralInt32((int32_t)(base + layout->offset))),
                compile_node(f->data.assign.right)));
        }
    }
    exprs.push_back(BinaryenConst(m_module, BinaryenLiteralInt32((int32_t)base)));
    return (uintptr_t)BinaryenBlock(m_module, nullptr, exprs.data(), exprs.size(), BinaryenTypeInt32());
}
```

- [ ] **Step 9: 实现字段读取与字段赋值**

```cpp
uintptr_t WasmCodegen::compile_member_access(ASTNode *node) {
    ASTNode *object = node->data.member_access.object;
    ASTNode *field = node->data.member_access.field;
    if (!object || !field || field->type != AST_IDENTIFIER || !field->data.identifier.name) {
        return (uintptr_t)BinaryenNop(m_module);
    }
    uintptr_t base = compile_node(object);
    // 按 object->inferred_type 或 struct literal 名称找到布局
    // 计算 field offset 后执行 emit_i32_load
    return base;
}
```

对 `p.x = 10`：在 `compile_assign()` 检测左值为 `AST_MEMBER_ACCESS`，走地址计算 + store。

- [ ] **Step 10: 运行测试，确认 struct fixture 可 emit**

Run: `cmake --build build --target test_wasm_codegen && "build/test_wasm_codegen.exe"`

Expected:

- `test_fixture_structs` PASS
- `test_fixture_binary_search` 至少通过 frontend + emit 阶段

- [ ] **Step 11: Commit**

```bash
git add src/compiler/WasmTypeMap.h src/compiler/WasmTypeMap.cpp src/compiler/WasmCodegen.h src/compiler/WasmCodegen.cpp tests/test_wasm_codegen.cpp
git commit -m "feat: add browser core wasm struct layout and field access"
```

### Task 6: 浏览器集成、运行期验证与示例收口

**Files:**

- Modify: `playground/playground.js`
- Modify: `playground/playground.html`
- Modify: `WebSite/playground/playground.js`
- Modify: `WebSite/playground/index.html`
- Modify: `playground/vixc-wasm.cpp`
- Modify: `tests/test_wasm_codegen.cpp`

**Interfaces:**

- Consumes: Task 1-5 的 Wasm emit 成果
- Produces:
  - Playground 示例集：`hello`、`bubble`、`struct`、`binary-search`
  - 浏览器运行期日志：Wasm 导出、实例化失败、`main()` 调用失败
  - 端到端 smoke checklist

- [ ] **Step 1: 增加最终 fixture 测试入口，覆盖所有 browser core fixtures**

```cpp
int main() {
    fprintf(stderr, "=== WASM Codegen Test ===\n");
    test_binaryen_basic();
    test_compile_to_wasm();
    test_string_literal_embedded_in_wasm();
    test_parse_error_reports_message();
    test_fixture_hello();
    test_compile_function_params_and_locals();
    test_fixture_control_flow();
    test_fixture_arrays();
    test_fixture_structs();
    test_fixture_bubble_sort();
    test_fixture_binary_search();
    fprintf(stderr, "\n%d passed, %d failed\n", tests_passed, tests_failed);
    return tests_failed > 0 ? 1 : 0;
}
```

- [ ] **Step 2: 在 `playground/playground.js` 固化最终示例集**

```js
var EXAMPLES = {
    hello: 'fn main(): i32 {\n'
        + '    print("Hello, Vix Playground!")\n'
        + '    return 0\n'
        + '}',
    bubble: '...bubble sort fixture...',
    struct: '...struct fixture...',
    'binary-search': '...binary search fixture...'
};
```

- [ ] **Step 3: 在浏览器运行路径保留关键诊断日志**

```js
console.log('[playground] compile ok, wasm size:', wasmBytes.length);
console.log('[playground] instantiate imports:', Object.keys(importObject.env));
console.log('[playground] exports:', Object.keys(instance.exports));
```

- [ ] **Step 4: 在 `playground/vixc-wasm.cpp` 保持错误兜底不为空**

```cpp
if (!ok) {
    if (error_msg.empty()) {
        error_msg = "compile failed";
    }
    if (error_out) {
        *error_out = strdup(error_msg.c_str());
    }
    return 0;
}
```

- [ ] **Step 5: 同步网站目录的示例与 JS**

Run: `cp "E:/Desktop/code/vix/Vix-lang/playground/playground.js" "E:/Desktop/code/vix/WebSite/playground/playground.js"`

Expected:

- `WebSite/playground/playground.js` 与本地版本一致

- [ ] **Step 6: 运行本地回归测试**

Run: `cmake --build build --target test_wasm_codegen && "build/test_wasm_codegen.exe"`

Expected:

- 所有 `tests/test_wasm_codegen.cpp` 用例 PASS

- [ ] **Step 7: 浏览器 smoke test**

Run:

```bash
cmake --build playground/build_wasm --target vixc-wasm
```

手工验证清单：

- 打开 `playground/playground.html`
- 选择 `Hello World`，点击 Run，页面输出正确字符串
- 选择 `Struct`，点击 Run，输出 `10` 与 `4`
- 选择 `Bubble Sort`，点击 Run，输出有序序列
- 选择 `Binary Search`，点击 Run，输出 `5`、`7`、`-1` 或对应索引结果
- 浏览器控制台无 `Aborted(undefined)`
- 浏览器控制台无 `not enough arguments on the stack for local.set`

- [ ] **Step 8: Commit**

```bash
git add playground/playground.js playground/playground.html WebSite/playground/playground.js WebSite/playground/index.html playground/vixc-wasm.cpp tests/test_wasm_codegen.cpp
git commit -m "feat: ship browser core wasm playground subset"
```

---

## 风险清单

### 1. `AST_EXPRESSION_LIST` 既可能是数组字面量，也可能是调用参数列表

- 规避：只在 `node->inferred_type` 明确为数组时走 `compile_array_literal()`
- 其余情况保持原有参数列表 / block 行为

### 2. Binaryen block/loop label 使用不当会生成非法 wasm

- 规避：每次新增控制流后都先跑 `test_wasm_codegen.exe`
- 实例化失败时保留浏览器端 `exports` / `instantiate` 日志

### 3. struct 布局与 typecheck 推断不一致

- 规避：初版只支持 `i32/bool/pointer-like` 字段，统一按 4 字节处理
- 暂不在 browser core 子集里引入复杂嵌套聚合字段

### 4. bump allocator 只增不减

- 规避：对子集 playground 可接受
- 文档中明确说明当前浏览器子集不提供释放语义

### 5. frontend 能过但 emit 阶段仍可能 silent wrong-code

- 规避：fixture 必须覆盖 `bubble sort`、`binary search`、`struct read/write`

---

## 完成定义

满足以下条件才算完成：

1. `tests/test_wasm_codegen.cpp` 中所有 browser core fixtures 均 PASS
2. 浏览器中 `hello`、`struct`、`bubble sort`、`binary search` 四个示例能编译、实例化、运行
3. 无空错误消息、无实例化级别非法 wasm 错误
4. `docs/browser-core-wasm-runtime.md` 已准确描述支持边界与运行时表示
5. Playground 明确体现这是“Browser Core 子集”，而不是完整 Vix 运行时

---

## Self-Review

- Spec coverage: 本计划覆盖了你确认的所有范围：浏览器内运行、只输出、不含 import/标准库/网络/文件/extern C、支持控制流/数组/struct
- Placeholder scan: 没有使用 `TODO`/`TBD`/“后续补充”这类空洞占位；风险项已单独列出
- Type consistency: 计划内新增接口统一围绕 `WasmCodegen`、`WasmTypeMap`、`tests/test_wasm_codegen.cpp` 展开，数组与 struct 都按 pointer-like 聚合值建模

## Execution Handoff

Plan complete and saved to `docs/superpowers/plans/2026-07-01-browser-core-wasm-backend.md`. Two execution options:

**1. Subagent-Driven (recommended)** - I dispatch a fresh subagent per task, review between tasks, fast iteration

**2. Inline Execution** - Execute tasks in this session using executing-plans, batch execution with checkpoints

Which approach?
