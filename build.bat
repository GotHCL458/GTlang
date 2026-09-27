@echo off
REM gtc unified build script
setlocal EnableDelayedExpansion
cd /d "%~dp0"
set "ROOT=%~dp0"
set "PATH=%ROOT%toolchain\rust\bin;%ROOT%toolchain\llvm\bin;%PATH%"
set "CARGO_HOME=%ROOT%.cargo-home"
set "RUSTUP_HOME=%ROOT%.rustup-home"
set "GTC_CLANG=%ROOT%toolchain\llvm\bin\clang.exe"
set "GTC_TCC=%ROOT%toolchain\tcc"
set "RUSTC=%ROOT%toolchain\rust\bin\rustc.exe"
set "MODE=%~1"
if "%MODE%"=="" set "MODE=build"
if /i "%MODE%"=="build"  goto do_build
if /i "%MODE%"=="debug"  goto do_debug
if /i "%MODE%"=="test"   goto do_test
if /i "%MODE%"=="clean"  goto do_clean
if /i "%MODE%"=="pack"   goto do_pack
echo Usage: build.bat [build^|debug^|test^|clean^|pack]
exit /b 1
:do_build
cargo build --release || exit /b 1
call :build_stdlib || exit /b 1
call :assemble_res || exit /b 1
echo [OK] artifacts assembled into %ROOT%res\
exit /b 0
:do_debug
cargo build || exit /b 1
call :build_stdlib || exit /b 1
call :assemble_res || exit /b 1
exit /b 0
:build_stdlib
set "STDOUT=%ROOT%.build\stdlib"
if exist "%STDOUT%" rmdir /s /q "%STDOUT%"
mkdir "%STDOUT%" 2>nul
for %%M in (math string) do (
    echo [stdlib] %%M
    "%RUSTC%" --edition 2021 --crate-type cdylib --crate-name %%M "%ROOT%src\stdlib\%%M.rs" -o "%STDOUT%\%%M.dll" -O || exit /b 1
    "%RUSTC%" --edition 2021 --crate-type staticlib --crate-name %%M "%ROOT%src\stdlib\%%M.rs" -o "%STDOUT%\%%M.lib" -O || exit /b 1
)
exit /b 0
:assemble_res
set "RES=%ROOT%res"
if exist "%RES%" rmdir /s /q "%RES%"
mkdir "%RES%" 2>nul
mkdir "%RES%\llvm\bin" 2>nul
mkdir "%RES%\tcc" 2>nul
mkdir "%RES%\runtime" 2>nul
mkdir "%RES%\lib" 2>nul
copy /Y "%ROOT%target\release\gtc.exe" "%RES%\gtc.exe" >nul || (echo [ERROR] gtc.exe not found & exit /b 1)
copy /Y "%ROOT%target\release\gtfmt.exe" "%RES%\gtfmt.exe" >nul
copy /Y "%ROOT%.build\stdlib\math.dll" "%RES%\lib\math.dll" >nul
copy /Y "%ROOT%.build\stdlib\string.dll" "%RES%\lib\string.dll" >nul
copy /Y "%ROOT%.build\stdlib\math.lib" "%RES%\lib\math.lib" >nul
copy /Y "%ROOT%.build\stdlib\string.lib" "%RES%\lib\string.lib" >nul
copy /Y "%ROOT%toolchain\llvm\bin\clang.exe" "%RES%\llvm\bin\clang.exe" >nul
copy /Y "%ROOT%toolchain\llvm\bin\lld-link.exe" "%RES%\llvm\bin\lld-link.exe" >nul
copy /Y "%ROOT%toolchain\tcc\libtcc.dll" "%RES%\tcc\libtcc.dll" >nul
)
xcopy /E /I /Y /Q "%ROOT%toolchain\tcc\include" "%RES%\tcc\include" >nul
xcopy /E /I /Y /Q "%ROOT%toolchain\tcc\lib" "%RES%\tcc\lib" >nul
copy /Y "%ROOT%src\runtime\gt_rt.c" "%RES%\runtime\gt_rt.c" >nul
"%ROOT%toolchain\llvm\bin\clang.exe" -c "%ROOT%src\runtime\gt_rt.c" -o "%RES%\runtime\gt_rt.obj" -O2 || exit /b 1
"%ROOT%toolchain\llvm\bin\lld-link.exe" /lib /out:"%RES%\lib\gt_rt.lib" "%RES%\runtime\gt_rt.obj" || exit /b 1
del /q "%RES%\runtime\gt_rt.obj" 2>nul
exit /b 0
:do_pack
call :do_build || exit /b 1
powershell -NoProfile -Command "if (Test-Path '%ROOT%res.zip') { Remove-Item '%ROOT%res.zip' -Force }; Compress-Archive -Path '%ROOT%res\*' -DestinationPath '%ROOT%res.zip' -CompressionLevel Optimal"
exit /b 0
:do_clean
cargo clean
if exist "%ROOT%res" rmdir /s /q "%ROOT%res"
if exist "%ROOT%.build" rmdir /s /q "%ROOT%.build"
if exist "%ROOT%res.zip" del /q "%ROOT%res.zip"
exit /b 0
:do_test
cargo build --release || exit /b 1
cargo test --release --quiet
exit /b 0
