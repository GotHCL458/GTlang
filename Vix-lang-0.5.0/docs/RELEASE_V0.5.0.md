# Vix Language v0.5.0 Release Notes

## Release Date: August 22, 2026

## Overview

更新 pipe macro，以及一些修复。

## Bug Fixes

- 更新 pipeline macro
- Windows 版本打包 MinGW 运行时库，`vixc main.vix` 无需单独安装 MinGW 即可链接
- 修复 `-obj` / `-S` 未指定 `-o` 时 "could not determine LLVM IR output path" 的问题
- 版本号更新至 0.5.0
