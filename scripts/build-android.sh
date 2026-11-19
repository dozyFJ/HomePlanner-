#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."

: "${ANDROID_NDK_HOME:?Set ANDROID_NDK_HOME to the Android NDK directory}"
: "${ANDROID_HOME:?Set ANDROID_HOME to the Android SDK directory}"
: "${HOMEPLANNER_KEYSTORE:?Set HOMEPLANNER_KEYSTORE to the OTA signing keystore}"
: "${HOMEPLANNER_KEYSTORE_PASSWORD:?Set HOMEPLANNER_KEYSTORE_PASSWORD}"
export HOMEPLANNER_KEYSTORE HOMEPLANNER_KEYSTORE_PASSWORD

case "$(uname -s)" in
    Linux) host=linux-x86_64 ;;
    Darwin) host=darwin-x86_64 ;;
    *) echo "Run the Android build on Linux or macOS." >&2; exit 1 ;;
esac
toolchain="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/$host/bin"
export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$toolchain/aarch64-linux-android26-clang"
export CC_aarch64_linux_android="$CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER"
export AR_aarch64_linux_android="$toolchain/llvm-ar"
export CARGO_TARGET_AARCH64_LINUX_ANDROID_RUSTFLAGS="-C link-arg=-Wl,-z,max-page-size=16384"

export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$PWD/target}"
cargo build --locked --release --lib --target aarch64-linux-android
stage=android/app/build/rust-libs/arm64-v8a
install -d "$stage"
install -m 644 "$CARGO_TARGET_DIR/aarch64-linux-android/release/libwolfcraft_home_planner.so" "$stage/"
"${GRADLE:-gradle}" -p android --no-daemon --console=plain :app:assembleRelease
