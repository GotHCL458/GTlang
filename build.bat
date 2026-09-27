@echo off
REM ============================================================
REM GTLang build script (system toolchain)
REM
REM Requirements (must be on PATH):
REM   - Rust toolchain (cargo)
REM   - LLVM / clang   (or set GTC_CLANG to clang.exe)
REM   - TCC            (optional, for inline C; set GTC_TCC to its dir)
REM
REM Usage:
REM   build.bat            build release binaries
REM   build.bat debug      build debug binaries
REM   build.bat test       run the test suite
REM   build.bat clean      clean build artifacts
REM
REM For packaging a portable res/ directory, see build_res.bat.
REM ============================================================
setlocal EnableDelayedExpansion
cd /d "%~dp0"
set "ROOT=%~dp0"

REM --- locate clang (honor GTC_CLANG, else use PATH) ---
if not "%GTC_CLANG%"=="" goto clang_ready
for /f "delims=" %%C in ('where clang 2^>nul') do set "GTC_CLANG=%%C"
if "%GTC_CLANG%"=="" echo [WARN] clang not found on PATH; set GTC_CLANG to clang.exe
:clang_ready

set "MODE=%~1"
if "%MODE%"=="" set "MODE=build"
if /i "%MODE%"=="build" goto do_build
if /i "%MODE%"=="debug" goto do_debug
if /i "%MODE%"=="test"  goto do_test
if /i "%MODE%"=="clean" goto do_clean
echo Usage: build.bat [build^|debug^|test^|clean]
exit /b 1

:do_build
cargo build --release || exit /b 1
call :build_stdlib || exit /b 1
echo [OK] release binaries + stdlib built
exit /b 0

:do_debug
cargo build || exit /b 1
call :build_stdlib || exit /b 1
echo [OK] debug binaries + stdlib built
exit /b 0

:build_stdlib
set "STDOUT=%ROOT%.build\stdlib"
if exist "%STDOUT%" rmdir /s /q "%STDOUT%"
mkdir "%STDOUT%" 2>nul
for %%M in (math string) do (
    echo [stdlib] %%M
    rustc --edition 2021 --crate-type cdylib --crate-name %%M "%ROOT%src\stdlib\%%M.rs" -o "%STDOUT%\%%M.dll" -O || exit /b 1
    rustc --edition 2021 --crate-type staticlib --crate-name %%M "%ROOT%src\stdlib\%%M.rs" -o "%STDOUT%\%%M.lib" -O || exit /b 1
)
exit /b 0

:do_test
cargo build --release || exit /b 1
cargo test --release --quiet
exit /b 0

:do_clean
cargo clean
if exist "%ROOT%.build" rmdir /s /q "%ROOT%.build"
if exist "%ROOT%res" rmdir /s /q "%ROOT%res"
exit /b 0
