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
REM   5. Assemble a portable res/ directory (gtc + gtfmt + stdlib +
REM      system clang/lld) ready to distribute.
REM
REM Requirements (on PATH, or set the env var):
REM   cargo / rustc    -> https://rustup.rs
REM   clang            -> https://releases.llvm.org   (or set GTC_CLANG)
REM
REM Note: TCC (for inline C) is optional and not bundled here.
REM       Set GTC_TCC to a TCC directory to include it.
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

set "CLANG_DIR=%~dp0"
for %%I in ("%GTC_CLANG%") do set "CLANG_DIR=%%~dpI"
set "LLD_LINK=%CLANG_DIR%lld-link.exe"
if not exist "%LLD_LINK%" (
    echo [WARN] lld-link.exe not found next to clang; res/llvm/bin will be incomplete
)

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

REM ---------- 5. assemble res/ ----------
echo.
echo [res] assembling portable res/ ...
set "RES=%ROOT%res"
if exist "%RES%" rmdir /s /q "%RES%"
mkdir "%RES%" 2>nul
mkdir "%RES%\llvm\bin" 2>nul
mkdir "%RES%\runtime" 2>nul
mkdir "%RES%\lib" 2>nul

copy /Y "%ROOT%target\release\gtc.exe"   "%RES%\gtc.exe"   >nul || (echo [ERROR] gtc.exe not found & exit /b 1)
copy /Y "%ROOT%target\release\gtfmt.exe" "%RES%\gtfmt.exe" >nul
copy /Y "%STDOUT%\math.dll"   "%RES%\lib\math.dll"   >nul
copy /Y "%STDOUT%\string.dll" "%RES%\lib\string.dll" >nul
copy /Y "%STDOUT%\math.lib"   "%RES%\lib\math.lib"   >nul
copy /Y "%STDOUT%\string.lib" "%RES%\lib\string.lib" >nul

copy /Y "%GTC_CLANG%" "%RES%\llvm\bin\clang.exe" >nul
if exist "%LLD_LINK%" copy /Y "%LLD_LINK%" "%RES%\llvm\bin\lld-link.exe" >nul

REM ---------- TCC (optional, for inline C) ----------
REM Search order: GTC_TCC env -> .\tcc -> .\toolchain\tcc -> tcc on PATH
if "%GTC_TCC%"=="" if exist "%ROOT%tcc\libtcc.dll" set "GTC_TCC=%ROOT%tcc"
if "%GTC_TCC%"=="" if exist "%ROOT%toolchain\tcc\libtcc.dll" set "GTC_TCC=%ROOT%toolchain\tcc"
if "%GTC_TCC%"=="" (
    for /f "delims=" %%T in ('where tcc 2^>nul') do set "GTC_TCC=%%~dpT"
)
if "%GTC_TCC%"=="" (
    echo [WARN] TCC not found; inline C blocks will be unavailable.
    echo        Set GTC_TCC to a TCC directory, or place one at .\toolchain\tcc
) else (
    if not exist "%GTC_TCC%\libtcc.dll" (
        echo [WARN] %GTC_TCC% has no libtcc.dll; inline C will be unavailable.
    ) else (
        echo [check] tcc %GTC_TCC%
        mkdir "%RES%\tcc" 2>nul
        copy /Y "%GTC_TCC%\libtcc.dll" "%RES%\tcc\libtcc.dll" >nul
        if exist "%GTC_TCC%\include" xcopy /E /I /Y /Q "%GTC_TCC%\include" "%RES%\tcc\include" >nul
        if exist "%GTC_TCC%\lib"     xcopy /E /I /Y /Q "%GTC_TCC%\lib"     "%RES%\tcc\lib"     >nul
    )
)

copy /Y "%ROOT%src\runtime\gt_rt.c" "%RES%\runtime\gt_rt.c" >nul
"%GTC_CLANG%" -c "%ROOT%src\runtime\gt_rt.c" -o "%RES%\runtime\gt_rt.obj" -O2
if errorlevel 1 ( echo [ERROR] gt_rt.c compile failed. & exit /b 1 )
if exist "%LLD_LINK%" (
    "%LLD_LINK%" /lib /out:"%RES%\lib\gt_rt.lib" "%RES%\runtime\gt_rt.obj" >nul
    if errorlevel 1 ( echo [WARN] gt_rt.lib creation failed. )
)
REM keep gt_rt.obj (may still be locked by the just-run compiler)

echo.
echo [OK] Build complete.
echo      gtc   : %ROOT%target\release\gtc.exe
echo      gtfmt : %ROOT%target\release\gtfmt.exe
echo      stdlib: %STDOUT%
echo      res   : %RES%\
