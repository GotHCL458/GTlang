@echo off
REM ============================================================
REM GTLang build script
REM
REM Just run it -- no arguments needed.
REM
REM   1. Check Rust ^(^>= 1.75^) and LLVM/clang ^(^>= 15^) on PATH.
REM   2. Build gtc (release).
REM   3. Build the standard library (src\gtlib\*.rs -> *.dll only; no .lib).
REM      Build GTO concurrency framework -> res\lib\gto.dll.
REM   4. Assemble a portable res\ directory.
REM
REM Env: GTC_CLANG (clang.exe path).
REM ============================================================
setlocal EnableDelayedExpansion
cd /d "%~dp0"
set "ROOT=%~dp0"

echo === GTLang build ===
echo.

REM ---------- 1. Rust ----------
where cargo >nul 2>nul
if errorlevel 1 (
    echo [ERROR] cargo not found on PATH. Install Rust from https://rustup.rs
    exit /b 1
)
for /f "tokens=2" %%V in ('cargo --version') do set "CARGO_VER=%%V"
echo [check] cargo %CARGO_VER%

REM ---------- 2. LLVM / clang ----------
if "%GTC_CLANG%"=="" (
    for /f "delims=" %%C in ('where clang 2^>nul') do set "GTC_CLANG=%%C"
)
if "%GTC_CLANG%"=="" (
    echo [ERROR] clang not found on PATH. Install LLVM ^(^>= 15^) or set GTC_CLANG.
    exit /b 1
)
for /f "delims=" %%L in ('"%GTC_CLANG%" --version 2^>nul') do (
    for /f "tokens=3" %%V in ("%%L") do set "CLANG_VER=%%V"
    goto :clang_done
)
:clang_done
echo [check] clang %CLANG_VER%

set "CLANG_DIR=%~dp0"
for %%I in ("%GTC_CLANG%") do set "CLANG_DIR=%%~dpI"
set "LLD_LINK=%CLANG_DIR%lld-link.exe"

echo.

REM ---------- 3. binaries ----------
echo [build] cargo build --release
cargo build --release
if errorlevel 1 exit /b 1

REM ---------- 4. gtlib ----------
set "STDOUT=%ROOT%.build\gtlib"
if exist "%STDOUT%" rmdir /s /q "%STDOUT%"
mkdir "%STDOUT%" 2>nul
REM Rust gtlib modules
for %%F in ("%ROOT%src\gtlib\*.rs") do (
    set "MOD=%%~nF"
    if not "!MOD!"=="mod" (
        echo [build] gtlib !MOD! (rust)
        rustc --edition 2021 --crate-type cdylib --crate-name !MOD! "%%~fF" -o "%STDOUT%\!MOD!.dll" -O
        if errorlevel 1 ( echo [ERROR] gtlib !MOD! dll failed. & exit /b 1 )
    )
)
REM C gtlib modules
for %%F in ("%ROOT%src\gtlib\*.c") do (
    set "MOD=%%~nF"
    echo [build] gtlib !MOD! ^(c^)
    set "CFILE=%ROOT%src\gtlib\!MOD!.c"
    set "COBJ=%STDOUT%\!MOD!.obj"
    "%GTC_CLANG%" -c -O2 -o "!COBJ!" "!CFILE!"
    if errorlevel 1 ( echo [ERROR] gtlib !MOD! ^(c^) compile failed. & exit /b 1 )
    "%GTC_CLANG%" -shared -O2 -o "%STDOUT%\!MOD!.dll" "!COBJ!"
    if errorlevel 1 ( echo [ERROR] gtlib !MOD! ^(c^) dll failed. & exit /b 1 )
    del /q "!COBJ!" 2>nul
)

REM ---------- 5. res ----------
echo.
echo [res] assembling portable res/ ...
set "RES=%ROOT%res"
if exist "%RES%" rmdir /s /q "%RES%" 2>nul
if exist "%RES%" cmd /c rmdir /s /q "%RES%" 2>nul
if exist "%RES%" (
    REM 目录可能被"某进程的 cwd"占用（如编辑器/终端）；尽力清空内容后继续覆盖。
    del /q /s "%RES%\*" 2>nul
    echo [warn] res/ could not be fully removed; overwriting in place.
)
mkdir "%RES%" 2>nul
mkdir "%RES%\llvm\bin" 2>nul
mkdir "%RES%\runtime" 2>nul
mkdir "%RES%\lib" 2>nul

copy /Y "%ROOT%target\release\gtc.exe"   "%RES%\gtc.exe"   >nul || (echo [ERROR] gtc.exe missing & exit /b 1)
copy /Y "%STDOUT%\*.dll" "%RES%\lib\" >nul 2>nul

copy /Y "%GTC_CLANG%" "%RES%\llvm\bin\clang.exe" >nul
if exist "%LLD_LINK%" copy /Y "%LLD_LINK%" "%RES%\llvm\bin\lld-link.exe" >nul

copy /Y "%ROOT%src\runtime\gt_rt.c" "%RES%\runtime\gt_rt.c" >nul
copy /Y "%ROOT%src\runtime\gc.c"   "%RES%\runtime\gc.c"   >nul
copy /Y "%ROOT%src\runtime\gc.h"   "%RES%\runtime\gc.h"   >nul
REM 运行时对象直接编到 res（产物不再链接 gt_rt.lib，而是直接编进可执行文件）
"%GTC_CLANG%" -c "%ROOT%src\runtime\gt_rt.c" -o "%RES%\runtime\gt_rt.obj" -O2 -fms-runtime-lib=libcmt
if errorlevel 1 ( echo [ERROR] gt_rt.c compile failed. & exit /b 1 )

REM ---------- GTO 并发框架（编译为 res\lib\gto.dll，供 AOT/JIT 运行期动态加载） ----------
echo [build] GTO ^(gto.dll^)
if exist "%ROOT%src\GTO\gto.c" (
    "%GTC_CLANG%" -c -O2 -fms-runtime-lib=libcmt -DGTO_BUILD_DLL -o "%RES%\runtime\gto.obj" "%ROOT%src\GTO\gto.c"
    if errorlevel 1 ( echo [ERROR] gto.c compile failed. & exit /b 1 )
    "%GTC_CLANG%" -shared -O2 -fms-runtime-lib=libcmt -o "%RES%\lib\gto.dll" "%RES%\runtime\gto.obj"
    if errorlevel 1 ( echo [ERROR] gto.dll link failed. & exit /b 1 )
    del /q "%RES%\runtime\gto.obj" 2>nul
) else (
    echo [WARN] src\GTO\gto.c not found; concurrency (go/chan) will fall back to built-in impl.
)

echo.
echo [OK] Build complete.
echo      res : %RES%\

