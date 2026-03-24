@echo off
REM Build script for Android APK
REM Requirements: ANDROID_NDK_ROOT environment variable set, cargo-ndk installed
REM Usage: build-android.cmd [debug|release]
REM
REM To sign release builds locally, create android\key.properties with:
REM   storeFile=../release.jks
REM   storePassword=your_store_password
REM   keyAlias=release
REM   keyPassword=your_key_password
REM
REM Generate a keystore with:
REM   keytool -genkey -v -keystore android\release.jks -keyalg RSA -keysize 2048 -validity 10000 -alias release

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
    if exist android\app\build\outputs\apk\release\app-release.apk (
        echo APK (signed): android\app\build\outputs\apk\release\app-release.apk
    ) else (
        echo APK (unsigned): android\app\build\outputs\apk\release\app-release-unsigned.apk
        echo To sign, create android\key.properties (see instructions at top of this script^)
    )
) else (
    echo APK: android\app\build\outputs\apk\debug\app-debug.apk
)
