@echo off
setlocal

set "INSTALL_DIR=%ProgramFiles%\Octo"
set "SCRIPT_DIR=%~dp0"

:: Check for pre-built binary first, then build from source
if exist "%SCRIPT_DIR%octo.exe" (
    echo Using pre-built binary.
    set "BINARY=%SCRIPT_DIR%octo.exe"
) else if exist "%SCRIPT_DIR%target\release\octo.exe" (
    echo Using previously built binary.
    set "BINARY=%SCRIPT_DIR%target\release\octo.exe"
) else (
    echo Building Octo (release)...
    cargo build --release
    if errorlevel 1 (
        echo Build failed. Install Rust from https://rustup.rs/ or download a pre-built release.
        exit /b 1
    )
    set "BINARY=%SCRIPT_DIR%target\release\octo.exe"
)

echo Installing to %INSTALL_DIR%...
if not exist "%INSTALL_DIR%" mkdir "%INSTALL_DIR%"
copy /y "%BINARY%" "%INSTALL_DIR%\octo.exe"
copy /y "%SCRIPT_DIR%assets\octo.svg" "%INSTALL_DIR%\octo.svg"
copy /y "%SCRIPT_DIR%assets\octo.png" "%INSTALL_DIR%\octo.png"

:: Add to PATH via registry (current user)
echo Adding %INSTALL_DIR% to user PATH...
for /f "tokens=2*" %%A in ('reg query "HKCU\Environment" /v Path 2^>nul') do set "CURRENT_PATH=%%B"
echo %CURRENT_PATH% | findstr /i /c:"%INSTALL_DIR%" >nul
if errorlevel 1 (
    setx PATH "%CURRENT_PATH%;%INSTALL_DIR%"
)

:: Convert PNG to ICO if not already present
if not exist "%INSTALL_DIR%\octo.ico" (
    if exist "%INSTALL_DIR%\octo.png" (
        echo Converting icon...
        powershell -NoProfile -Command ^
            "Add-Type -AssemblyName System.Drawing; $bmp = [System.Drawing.Bitmap]::new('%INSTALL_DIR%\octo.png'); $ico = [System.IO.File]::Create('%INSTALL_DIR%\octo.ico'); $bmp.Save($ico, [System.Drawing.Imaging.ImageFormat]::Icon); $ico.Close(); $bmp.Dispose()"
    )
)

:: Create Start Menu shortcut
set "SHORTCUT_DIR=%APPDATA%\Microsoft\Windows\Start Menu\Programs"
echo Creating Start Menu shortcut...
if exist "%INSTALL_DIR%\octo.ico" (
    set "ICON_PATH=%INSTALL_DIR%\octo.ico"
) else (
    set "ICON_PATH=%INSTALL_DIR%\octo.exe"
)
powershell -NoProfile -Command "$ws = New-Object -ComObject WScript.Shell; $sc = $ws.CreateShortcut('%SHORTCUT_DIR%\Octo.lnk'); $sc.TargetPath = '%INSTALL_DIR%\octo.exe'; $sc.IconLocation = '%ICON_PATH%'; $sc.WorkingDirectory = '%USERPROFILE%'; $sc.Description = 'Multi-format data viewer and editor'; $sc.Save()"

echo.
echo Octo installed successfully.
echo   Binary:   %INSTALL_DIR%\octo.exe
echo   Shortcut: %SHORTCUT_DIR%\Octo.lnk
echo.
echo You may need to restart your terminal for PATH changes to take effect.
endlocal
