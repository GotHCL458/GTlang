@echo off
REM ============================================================
REM GTLang build script
REM
REM Just run it -- no arguments needed.
REM
REM   1. Check Rust ^(^>= 1.75^) and LLVM/clang ^(^>= 15^) on PATH.
REM   2. Build gtc + gtfmt (release).
REM   3. Build the standard library (src\gtlib\*.rs -> *.dll + *.lib).
REM   4. Assemble a portable res\ directory.
REM
REM Env: GTC_CLANG (clang.exe path), GTC_TCC (TCC dir).
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
        rustc --edition 2021 --crate-type staticlib --crate-name !MOD! "%%~fF" -o "%STDOUT%\!MOD!_static.lib" -O
        if errorlevel 1 ( echo [ERROR] gtlib !MOD! static failed. & exit /b 1 )
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
    del /q "!COBJ!" 2>nul
)

REM ---------- 5. res ----------
echo.
echo [res] assembling portable res/ ...
set "RES=%ROOT%res"
if exist "%RES%" rmdir /s /q "%RES%" 2>nul
if exist "%RES%" cmd /c rmdir /s /q "%RES%" 2>nul
if exist "%RES%" (
    echo [ERROR] cannot clean %RES% -- close any running gtc.exe/gtfmt.exe and retry.
    exit /b 1
)
mkdir "%RES%" 2>nul
mkdir "%RES%\llvm\bin" 2>nul
mkdir "%RES%\runtime" 2>nul
mkdir "%RES%\lib" 2>nul

copy /Y "%ROOT%target\release\gtc.exe"   "%RES%\gtc.exe"   >nul || (echo [ERROR] gtc.exe missing & exit /b 1)
copy /Y "%ROOT%target\release\gtfmt.exe" "%RES%\gtfmt.exe" >nul
copy /Y "%STDOUT%\*.dll" "%RES%\lib\" >nul 2>nul
if not exist "%RES%\lib\.lib" mkdir "%RES%\lib\.lib" 2>nul
copy /Y "%STDOUT%\*.lib" "%RES%\lib\.lib\" >nul 2>nul

copy /Y "%GTC_CLANG%" "%RES%\llvm\bin\clang.exe" >nul
if exist "%LLD_LINK%" copy /Y "%LLD_LINK%" "%RES%\llvm\bin\lld-link.exe" >nul

REM ---------- TCC ----------
if "%GTC_TCC%"=="" if exist "%ROOT%tcc\libtcc.dll" set "GTC_TCC=%ROOT%tcc"
if "%GTC_TCC%"=="" (
    for /f "delims=" %%T in ('where tcc 2^>nul') do set "GTC_TCC=%%~dpT"
)
if "%GTC_TCC%"=="" (
    echo [WARN] TCC not found; inline C blocks will be unavailable.
) else (
    if exist "%GTC_TCC%\libtcc.dll" (
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
if exist "%LLD_LINK%" "%LLD_LINK%" /lib /out:"%RES%\lib\gt_rt.lib" "%RES%\runtime\gt_rt.obj" >nul 2>nul

echo.
echo [OK] Build complete.
echo      res : %RES%\
