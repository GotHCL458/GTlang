# Vix 导入解析规则

## 概述

`import "路径"` 语句根据路径格式按不同优先级查找模块文件。搜索路径由 `VIX_HOME` 环境变量、当前工作目录及导入文件所在目录共同决定。

`VIX_HOME` 未设置时，包名展开后的 `$VIX_HOME` 相关路径会被跳过。

---

## 一、绝对路径

以 `/` 开头，直接检查文件是否存在。

| 示例 | 解析路径 |
|------|----------|
| `import "/abs/lib.vix"` | `/abs/lib.vix` |

---

## 二、相对路径（不含 `/` 开头的裸名）

不包含 `/` 的裸名（如 `import "os"`）先经**包名展开**处理。

### 包名展开规则

| 格式 | 展开结果 |
|------|----------|
| `name`（无点） | `github.com/vixlang/vlib-name` |
| `@name` | `gitee.com/vixlang/vlib-name` |
| `user.repo` | `github.com/user/repo` |
| `registry:user.repo` | `registry.com/user/repo` |

### 搜索顺序

展开为 `registry/user/repo` 后：

1. **`.vix/libs/<展开路径>/main.vix`** — 本地项目包
2. **`$VIX_HOME/libs/<展开路径>/main.vix`** — 全局包
3. **`$VIX_HOME/std/<裸名>.vix`** — 标准库单文件
4. **`$VIX_HOME/std/<裸名>/main.vix`** — 标准库目录

#### 示例：`import "os"`

| 优先级 | 路径 |
|--------|------|
| 1 | `.vix/libs/github.com/vixlang/vlib-os/main.vix` |
| 2 | `$VIX_HOME/libs/github.com/vixlang/vlib-os/main.vix` |
| 3 | `$VIX_HOME/std/os.vix` |
| 4 | `$VIX_HOME/std/os/main.vix` |

---

## 三、路径样式导入（包含 `/`）

含 `/` 的路径按以下目录顺序搜索：

1. **`$VIX_HOME/std/<路径>`**
2. **`.vix/libs/<路径>`**
3. **`$VIX_HOME/libs/<路径>`**

#### 示例：`import "net.vix"`

| 优先级 | 路径 |
|--------|------|
| 1 | `$VIX_HOME/std/net.vix` |
| 2 | `.vix/libs/net.vix` |
| 3 | `$VIX_HOME/libs/net.vix` |

#### 示例：`import "sub/mod.vix"`

| 优先级 | 路径 |
|--------|------|
| 1 | `$VIX_HOME/std/sub/mod.vix` |
| 2 | `.vix/libs/sub/mod.vix` |
| 3 | `$VIX_HOME/libs/sub/mod.vix` |

---

## 四、内部处理流程

1. 解析器识别 `import "路径"` → 创建 `AST_IMPORT` 节点
2. 内联阶段调用 `vix_resolve_import_path()` 定位文件
3. 若已在导入缓存中，跳过
4. 解析文件为 AST，递归处理其子导入
5. 提取该模块的**公有项**（`pub` 函数/常量/结构体/全局变量）替换 `import` 语句
