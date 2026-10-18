#!/data/data/com.termux/files/usr/bin/bash
set -euo pipefail

ROOT="$HOME/ZakoProxyCLI/ZakoProxyCLI"
MOBILE="$ROOT/mobile"
JARS="$HOME/apk_tools/extracted/data/data/com.termux/files/usr/share/java"
ANDROID_JAR="$HOME/apk_tools/android-28.jar"
KS="$HOME/apk_build/debug.keystore"
BIN="$ROOT/target/release/zakoai-proxy"

echo "==> [1/6] 同步前端产物与二进制"
rm -rf "$MOBILE/assets" "$MOBILE/lib"
mkdir -p "$MOBILE/assets" "$MOBILE/lib/arm64-v8a" "$MOBILE/lib/x86_64"
cp -r "$ROOT/web/dist" "$MOBILE/assets/web"
cp "$BIN" "$MOBILE/lib/arm64-v8a/libzakoai.so"
chmod 755 "$MOBILE/lib/arm64-v8a/libzakoai.so"
X86="$ROOT/target/x86_64-linux-android/release/zakoai-proxy"
if [ -f "$X86" ]; then
  cp "$X86" "$MOBILE/lib/x86_64/libzakoai.so"
  chmod 755 "$MOBILE/lib/x86_64/libzakoai.so"
  echo "x86_64 binary included"
else
  echo "WARN: x86_64 binary missing (run the x86_64 cargo build)"
fi
ls -lh "$MOBILE/lib/arm64-v8a/libzakoai.so" "$MOBILE/lib/x86_64/libzakoai.so" 2>/dev/null

echo "==> [2/6] javac"
rm -rf "$MOBILE/obj"; mkdir -p "$MOBILE/obj"
javac -source 11 -target 11 -cp "$ANDROID_JAR" \
  -d "$MOBILE/obj" $(find "$MOBILE/src" -name '*.java')

echo "==> [3/6] d8"
rm -rf "$MOBILE/dex"; mkdir -p "$MOBILE/dex"
java -cp "$JARS/d8.jar" com.android.tools.r8.D8 \
  --min-api 24 --lib "$ANDROID_JAR" --output "$MOBILE/dex" \
  $(find "$MOBILE/obj" -name '*.class')
ls -lh "$MOBILE/dex"

echo "==> [4/6] aapt package"
cd "$MOBILE"
rm -f base.apk aligned.apk ZakoAI-debug.apk
aapt package -f -M AndroidManifest.xml -S res -A assets -I "$ANDROID_JAR" -F base.apk

echo "==> [5/6] 注入 classes.dex 与原生库"
cp "$MOBILE/dex/classes.dex" "$MOBILE/classes.dex"
aapt add base.apk classes.dex
aapt add base.apk lib/arm64-v8a/libzakoai.so
if [ -f "$MOBILE/lib/x86_64/libzakoai.so" ]; then
  aapt add base.apk lib/x86_64/libzakoai.so
fi
rm -f "$MOBILE/classes.dex"

echo "==> [6/6] 对齐 + 签名"
zipalign -f 4 base.apk aligned.apk
java -jar "$JARS/apksigner.jar" sign \
  --ks "$KS" --ks-pass pass:android --key-pass pass:android \
  --ks-key-alias androiddebugkey --min-sdk-version 24 \
  --out ZakoAI-debug.apk aligned.apk

echo "==> 完成"
ls -lh "$MOBILE/ZakoAI-debug.apk"
