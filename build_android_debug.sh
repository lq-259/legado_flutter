#!/bin/bash
set -euo pipefail

# Android 构建脚本 —— 编译 Rust 原生库 + Flutter APK 并安装到设备
# 用法: bash build_android_debug.sh

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

export ANDROID_HOME="${ANDROID_HOME:-/opt/android-sdk}"
NDK_VER="28.2.13676358"
export NDK_HOME="$ANDROID_HOME/ndk/$NDK_VER"
NDK_BIN="$NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin"

export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$NDK_BIN/aarch64-linux-android21-clang"
export CC_aarch64_linux_android="$NDK_BIN/aarch64-linux-android21-clang"
export AR_aarch64_linux_android="$NDK_BIN/llvm-ar"
export FLUTTER_ROOT="${FLUTTER_ROOT:-/root/flutter}"
export PATH="$FLUTTER_ROOT/bin:$ANDROID_HOME/platform-tools:$PATH"

echo "[1/3] Cross-compiling Rust bridge crate..."
cargo build --manifest-path core/bridge/Cargo.toml --release --target aarch64-linux-android

echo "[2/3] Copying libbridge.so to jniLibs..."
cp core/target/aarch64-linux-android/release/libbridge.so \
   flutter_app/android/app/src/main/jniLibs/arm64-v8a/libbridge.so

echo "[3/3] Building Flutter debug APK and installing..."
cd flutter_app
flutter build apk --debug
adb install -r build/app/outputs/flutter-apk/app-debug.apk || adb install build/app/outputs/flutter-apk/app-debug.apk

echo ""
echo "Done! APK installed to device."
