# 贡献指南

感谢你对 Vix 编程语言的兴趣！本文档介绍如何参与 Vix 的开发。

## 环境准备

### 依赖

- **CMake** ≥ 3.16
- **LLVM** ≥ 14（含 clang）
- **C/C++ 编译器**：GCC ≥ 9、Clang ≥ 12 或 MSVC ≥ 2019

### 构建步骤

```bash
git clone https://github.com/vixlang/Vix-lang.git
cd Vix-lang
mkdir build && cd build
cmake ..
make -j$(nproc)
```

在 Windows 上使用 MSVC：

```cmd
mkdir build && cd build
cmake .. -G "Visual Studio 17 2022"
cmake --build . --config Release
```

## 项目结构

| 目录 | 说明 |
|------|------|
| `src/` | 编译器源码（词法分析、语法分析、类型系统、代码生成） |
| `include/` | 公共/内部头文件 |
| `examples/` | 语言示例程序 |
| `test/` | 回归测试和行为测试 |
| `Docs/` | 语言文档和发布说明 |

## 开发流程

1. **Fork** 仓库到你的 GitHub 账号
2. 从 `main` 创建特性分支：`git checkout -b feat/your-feature`
3. 编写代码，确保：
   - 代码风格与现有代码保持一致
   - 新增功能附带测试
   - 编译无警告
4. 提交：使用清晰的 commit message（`feat:`、`fix:`、`docs:`、`test:`、`refactor:` 前缀）
5. 推送到你的 fork 并创建 PR 到 `main` 分支

## 代码规范

- C/C++ 代码遵循现有风格（4 空格缩进、花括号换行）
- 新增头文件需放置在 `include/` 对应目录下
- 公共接口需要注释说明

## 测试

```bash
cd build
ctest --output-on-failure
```

新增功能或修复 bug 时请添加对应测试用例到 `test/` 目录。

## 报告问题

- 使用 [GitHub Issues](https://github.com/vixlang/Vix-lang/issues) 报告 bug 或提出功能建议
- Bug 报告请包含：复现步骤、预期行为、实际行为、环境信息（OS、编译器版本、LLVM 版本）

## 发布流程

发布由维护者执行，参考 `Docs/` 中的发布说明。贡献者不需要关心版本号和发布流程。

## 行为准则

请保持尊重和友好的态度。我们欢迎所有水平的贡献者。

---

如有疑问，欢迎在 [GitHub Issues](https://github.com/vixlang/Vix-lang/issues) 中提问。
