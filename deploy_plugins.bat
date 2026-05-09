@echo off
setlocal

set "SRC_ROOT=%~dp0target\bundled"
set "SRC_VST3=%SRC_ROOT%\Dispersion Equalizer.vst3"
set "SRC_CLAP=%SRC_ROOT%\Dispersion Equalizer.clap"

set "DST_VST3=C:\Program Files\Common Files\VST3\Dispersion Equalizer.vst3"
set "DST_CLAP=C:\Program Files\Common Files\CLAP\Dispersion Equalizer.clap"

echo === Build ^& Deploy Dispersion Equalizer ===

echo [1/3] Building VST3 and CLAP (release)...
cargo xtask bundle dispersion_equalizer --release
if errorlevel 1 (
    echo [ERROR] Build failed.
    exit /b 1
)
echo Build succeeded.

if not exist "%SRC_VST3%" (
    echo [ERROR] VST3 source not found: "%SRC_VST3%"
    exit /b 1
)

if not exist "%SRC_CLAP%" (
    echo [ERROR] CLAP source not found: "%SRC_CLAP%"
    exit /b 1
)

echo [2/3] Copying VST3 bundle...
robocopy "%SRC_VST3%" "%DST_VST3%" /E /R:2 /W:1 >nul
if errorlevel 8 (
    echo [ERROR] Failed to copy VST3 bundle.
    exit /b 1
)

echo [3/3] Copying CLAP plugin...
copy /Y "%SRC_CLAP%" "%DST_CLAP%" >nul
if errorlevel 1 (
    echo [ERROR] Failed to copy CLAP plugin.
    exit /b 1
)

echo Done.
exit /b 0
