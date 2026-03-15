@echo off
REM Build script for Android APK
REM Requirements: ANDROID_NDK_ROOT environment variable set, cargo-ndk installed
REM Usage: build-android.cmd [debug|release]

set BUILD_TYPE=%1
if "%BUILD_TYPE%"=="" set BUILD_TYPE=debug

echo Building Fractal Viewer for Android (%BUILD_TYPE%)...

REM Step 1: Build native libraries with cargo-ndk
echo [1/2] Building native Rust libraries...
if "%BUILD_TYPE%"=="release" (
    cargo ndk -t aarch64-linux-android -t armv7-linux-androideabi -t x86_64-linux-android --platform 30 -o android\app\src\main\jniLibs build -p fractal-app --lib --release
) else (
    cargo ndk -t aarch64-linux-android -t armv7-linux-androideabi -t x86_64-linux-android --platform 30 -o android\app\src\main\jniLibs build -p fractal-app --lib
)

if %ERRORLEVEL% NEQ 0 (
    echo ERROR: Native library build failed!
    exit /b 1
)

REM Step 2: Build APK with Gradle
echo [2/2] Building APK with Gradle...
cd android
if "%BUILD_TYPE%"=="release" (
    gradlew.bat assembleRelease
) else (
    gradlew.bat assembleDebug
)

if %ERRORLEVEL% NEQ 0 (
    echo ERROR: Gradle build failed!
    cd ..
    exit /b 1
)

cd ..

echo.
echo Build complete!
if "%BUILD_TYPE%"=="release" (
    echo APK: android\app\build\outputs\apk\release\app-release-unsigned.apk
) else (
    echo APK: android\app\build\outputs\apk\debug\app-debug.apk
)
