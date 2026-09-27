@echo off
REM ============================================================
REM GTLang build script
REM
REM Just run it -- no arguments needed.
REM
REM It will:
REM   1. Check that the system Rust toolchain (cargo/rustc) is available
REM      and new enough (>= 1.75).
REM   2. Check that a system LLVM/clang is available and new enough (>= 15).
REM   3. Build gtc + gtfmt with cargo (release).
REM   4. Build the standard library (math / string) with rustc.
REM
REM Requirements (on PATH, or set the env var):
REM   cargo / rustc    -> https://rustup.rs
REM   clang            -> https://releases.llvm.org   (or set GTC_CLANG)
REM
REM For packaging a portable res/ directory (with bundled LLVM/TCC),
REM use build_res.bat (local only, not shipped).
REM ============================================================
setlocal EnableDelayedExpansion
cd /d "%~dp0"
set "ROOT=%~dp0"

echo === GTLang build ===
echo.

REM ---------- 1. Rust ----------
where cargo >nul 2>nul
if errorlevel 1 (
    echo [ERROR] cargo not found on PATH.
    echo         Install Rust from https://rustup.rs and re-open the terminal.
    exit /b 1
)
for /f "tokens=2" %%V in ('cargo --version') do set "CARGO_VER=%%V"
echo [check] cargo %CARGO_VER%
powershell -NoProfile -Command "if ([version]('%CARGO_VER%') -lt [version]'1.75') { Write-Host '[ERROR] Rust >= 1.75 required (found %CARGO_VER%). Update with: rustup update'; exit 1 }"
if errorlevel 1 exit /b 1

REM ---------- 2. LLVM / clang ----------
if "%GTC_CLANG%"=="" (
    for /f "delims=" %%C in ('where clang 2^>nul') do set "GTC_CLANG=%%C"
)
if "%GTC_CLANG%"=="" (
    echo [ERROR] clang not found on PATH.
    echo         Install LLVM from https://releases.llvm.org
    echo         or set GTC_CLANG to the full path of clang.exe.
    exit /b 1
)
for /f "delims=" %%L in ('"%GTC_CLANG%" --version 2^>nul') do (
    for /f "tokens=3" %%V in ("%%L") do set "CLANG_VER=%%V"
    goto :clang_ver_done
)
:clang_ver_done
echo [check] clang %CLANG_VER%
powershell -NoProfile -Command "$m = [regex]::Match('%CLANG_VER%', '^[0-9]+'); if (-not $m.Success -or [int]$m.Value -lt 15) { Write-Host '[ERROR] LLVM/clang >= 15 required (found %CLANG_VER%).'; exit 1 }"
if errorlevel 1 exit /b 1

echo.

REM ---------- 3. build binaries ----------
echo [build] cargo build --release
cargo build --release
if errorlevel 1 (
    echo [ERROR] cargo build failed.
    exit /b 1
)

REM ---------- 4. build stdlib ----------
set "STDOUT=%ROOT%.build\stdlib"
if exist "%STDOUT%" rmdir /s /q "%STDOUT%"
mkdir "%STDOUT%" 2>nul
for %%M in (math string) do (
    echo [build] stdlib %%M
    rustc --edition 2021 --crate-type cdylib --crate-name %%M "%ROOT%src\stdlib\%%M.rs" -o "%STDOUT%\%%M.dll" -O
    if errorlevel 1 ( echo [ERROR] stdlib %%M failed. & exit /b 1 )
    rustc --edition 2021 --crate-type staticlib --crate-name %%M "%ROOT%src\stdlib\%%M.rs" -o "%STDOUT%\%%M.lib" -O
    if errorlevel 1 ( echo [ERROR] stdlib %%M failed. & exit /b 1 )
)

echo.
echo [OK] Build complete.
echo      gtc : %ROOT%target\release\gtc.exe
echo      gtfmt: %ROOT%target\release\gtfmt.exe
echo      stdlib: %STDOUT%
