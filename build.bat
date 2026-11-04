@echo off
setlocal
echo ===================================================
echo [Conduit] Fast Parallel Build (16 Threads)
echo ===================================================

cd /d "%~dp0"

:: Set environment path for Rust toolchain and MinGW compiler
set "USERPROFILE=%USERPROFILE%"
set "PATH=%USERPROFILE%\.cargo\bin;C:\Users\aleja\AppData\Local\Microsoft\WinGet\Packages\BrechtSanders.WinLibs.POSIX.UCRT_Microsoft.Winget.Source_8wekyb3d8bbwe\mingw64\bin;%PATH%"

:: Optimize build for maximum multithreading
set "CARGO_BUILD_JOBS=16"
set "RAYON_NUM_THREADS=16"

echo [*] Compiling Frontend (Vite + Vue 3)...
call npm run build
if %ERRORLEVEL% NEQ 0 (
    echo [ERROR] Frontend build failed!
    pause
    exit /b %ERRORLEVEL%
)

echo [*] Compiling Native Tauri Desktop App (Parallel)...
call npm run tauri build -- --no-bundle
if %ERRORLEVEL% NEQ 0 (
    echo [ERROR] Tauri native build failed!
    pause
    exit /b %ERRORLEVEL%
)

echo [*] Updating root conduit.exe and runtime libraries...
taskkill /F /IM conduit.exe >nul 2>&1
timeout /t 1 /nobreak >nul
copy /Y "src-tauri\target\release\conduit.exe" "conduit.exe" >nul
copy /Y "src-tauri\target\release\WebView2Loader.dll" "WebView2Loader.dll" >nul

echo ===================================================
echo [SUCCESS] Build complete! conduit.exe is ready.
echo ===================================================
pause
