@echo off
REM ============================================================
REM  update-icons.cmd — Generate all platform icons from source
REM  Requires: ImageMagick (magick) on PATH
REM  Source:    ModernFractalViewer.png (project root)
REM  Usage:     scripts\update-icons.cmd
REM ============================================================

setlocal enabledelayedexpansion

set SOURCE=ModernFractalViewer.png

REM Check source image exists
if not exist "%SOURCE%" (
    echo ERROR: Source image "%SOURCE%" not found in project root.
    echo        Run this script from the project root directory.
    exit /b 1
)

REM Check ImageMagick is available
magick --version >nul 2>&1
if %ERRORLEVEL% NEQ 0 (
    echo ERROR: ImageMagick ^(magick^) not found. Install from https://imagemagick.org
    exit /b 1
)

echo ============================================================
echo  Generating icons from %SOURCE%
echo ============================================================

REM ─────────────────────────────────────────────
REM  1. Windows / Desktop — ICO files
REM     icon.ico: multi-size (16, 24, 32, 48, 64, 128, 256)
REM     favicon.ico: multi-size (16, 32, 48)
REM ─────────────────────────────────────────────
echo.
echo [1/4] Generating Windows/Desktop ICO files...

set ASSETS=crates\fractal-app\assets

magick "%SOURCE%" ^
  ( -clone 0 -resize 16x16 ) ^
  ( -clone 0 -resize 24x24 ) ^
  ( -clone 0 -resize 32x32 ) ^
  ( -clone 0 -resize 48x48 ) ^
  ( -clone 0 -resize 64x64 ) ^
  ( -clone 0 -resize 128x128 ) ^
  ( -clone 0 -resize 256x256 ) ^
  -delete 0 ^
  "%ASSETS%\icon.ico"

if %ERRORLEVEL% NEQ 0 (
    echo ERROR: Failed to generate icon.ico
    exit /b 1
)
echo   Created %ASSETS%\icon.ico

magick "%SOURCE%" ^
  ( -clone 0 -resize 16x16 ) ^
  ( -clone 0 -resize 32x32 ) ^
  ( -clone 0 -resize 48x48 ) ^
  -delete 0 ^
  "%ASSETS%\favicon.ico"

if %ERRORLEVEL% NEQ 0 (
    echo ERROR: Failed to generate favicon.ico
    exit /b 1
)
echo   Created %ASSETS%\favicon.ico

REM ─────────────────────────────────────────────
REM  2. Web / WASM — PNG files
REM     icon.png: 256x256
REM     favicon.png: 32x32
REM ─────────────────────────────────────────────
echo.
echo [2/4] Generating Web/WASM PNG files...

magick "%SOURCE%" -resize 256x256 "%ASSETS%\icon.png"
echo   Created %ASSETS%\icon.png (256x256)

magick "%SOURCE%" -resize 32x32 "%ASSETS%\favicon.png"
echo   Created %ASSETS%\favicon.png (32x32)

REM ─────────────────────────────────────────────
REM  3. Android — Launcher icons (ic_launcher.png)
REM     mdpi:    48x48
REM     hdpi:    72x72
REM     xhdpi:   96x96
REM     xxhdpi:  144x144
REM     xxxhdpi: 192x192
REM ─────────────────────────────────────────────
echo.
echo [3/4] Generating Android launcher icons (ic_launcher.png)...

set ANDROID_RES=android\app\src\main\res

magick "%SOURCE%" -resize 48x48   "%ANDROID_RES%\mipmap-mdpi\ic_launcher.png"
echo   Created mipmap-mdpi\ic_launcher.png (48x48)

magick "%SOURCE%" -resize 72x72   "%ANDROID_RES%\mipmap-hdpi\ic_launcher.png"
echo   Created mipmap-hdpi\ic_launcher.png (72x72)

magick "%SOURCE%" -resize 96x96   "%ANDROID_RES%\mipmap-xhdpi\ic_launcher.png"
echo   Created mipmap-xhdpi\ic_launcher.png (96x96)

magick "%SOURCE%" -resize 144x144 "%ANDROID_RES%\mipmap-xxhdpi\ic_launcher.png"
echo   Created mipmap-xxhdpi\ic_launcher.png (144x144)

magick "%SOURCE%" -resize 192x192 "%ANDROID_RES%\mipmap-xxxhdpi\ic_launcher.png"
echo   Created mipmap-xxxhdpi\ic_launcher.png (192x192)

REM ─────────────────────────────────────────────
REM  4. Android — Adaptive icon foreground (ic_launcher_foreground.png)
REM     These need 108dp with the icon centered in the inner 72dp (66.67%%)
REM     mdpi:    108x108  (icon area: 72x72)
REM     hdpi:    162x162  (icon area: 108x108)
REM     xhdpi:   216x216  (icon area: 144x144)
REM     xxhdpi:  324x324  (icon area: 216x216)
REM     xxxhdpi: 432x432  (icon area: 288x288)
REM ─────────────────────────────────────────────
echo.
echo [4/4] Generating Android adaptive icon foregrounds (ic_launcher_foreground.png)...

magick "%SOURCE%" -resize 72x72   -gravity center -background none -extent 108x108   "%ANDROID_RES%\mipmap-mdpi\ic_launcher_foreground.png"
echo   Created mipmap-mdpi\ic_launcher_foreground.png (108x108)

magick "%SOURCE%" -resize 108x108 -gravity center -background none -extent 162x162   "%ANDROID_RES%\mipmap-hdpi\ic_launcher_foreground.png"
echo   Created mipmap-hdpi\ic_launcher_foreground.png (162x162)

magick "%SOURCE%" -resize 144x144 -gravity center -background none -extent 216x216   "%ANDROID_RES%\mipmap-xhdpi\ic_launcher_foreground.png"
echo   Created mipmap-xhdpi\ic_launcher_foreground.png (216x216)

magick "%SOURCE%" -resize 216x216 -gravity center -background none -extent 324x324   "%ANDROID_RES%\mipmap-xxhdpi\ic_launcher_foreground.png"
echo   Created mipmap-xxhdpi\ic_launcher_foreground.png (324x324)

magick "%SOURCE%" -resize 288x288 -gravity center -background none -extent 432x432   "%ANDROID_RES%\mipmap-xxxhdpi\ic_launcher_foreground.png"
echo   Created mipmap-xxxhdpi\ic_launcher_foreground.png (432x432)

echo.
echo ============================================================
echo  All icons generated successfully!
echo ============================================================
echo.
echo  Summary:
echo    Windows:  %ASSETS%\icon.ico (16-256px multi-size)
echo    Favicon:  %ASSETS%\favicon.ico (16-48px multi-size)
echo    Web PNG:  %ASSETS%\icon.png (256px), favicon.png (32px)
echo    Android:  %ANDROID_RES%\mipmap-*\ic_launcher.png
echo              %ANDROID_RES%\mipmap-*\ic_launcher_foreground.png
echo.
