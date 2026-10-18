#!/data/data/com.termux/files/usr/bin/bash
# 交叉编译 x86_64 版二进制（供 Android x86_64 模拟器使用）
# 依赖：Termux 的 rust + rust-std-x86-64-linux-android、ndk-x86 sysroot
set -euo pipefail
cd "$HOME/ZakoProxyCLI/ZakoProxyCLI"
export CARGO_TARGET_X86_64_LINUX_ANDROID_LINKER="$HOME/ndk-x86/clang-x86_64.sh"
export CC_x86_64_linux_android="$HOME/ndk-x86/clang-x86_64.sh"
export AR_x86_64_linux_android="llvm-ar"
cargo build --release --target x86_64-linux-android --bin zakoai-proxy
echo "OK: target/x86_64-linux-android/release/zakoai-proxy"
