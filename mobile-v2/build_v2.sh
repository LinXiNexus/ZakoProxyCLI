#!/data/data/com.termux/files/usr/bin/bash
set -euo pipefail
ROOT="$HOME/ZakoProxyCLI/ZakoProxyCLI"
V2="$ROOT/mobile-v2"
JARS="$HOME/apk_tools/extracted/data/data/com.termux/files/usr/share/java"
ANDROID_JAR="$HOME/apk_tools/android-28.jar"
KS="$HOME/apk_build/debug.keystore"

echo "==> [1/6] aapt package + R.java"
cd "$V2"
rm -rf build; mkdir -p build
aapt package -f -M AndroidManifest.xml -S res -I "$ANDROID_JAR" \
  -J build -F build/base.apk

echo "==> [2/6] javac"
mkdir -p build/obj
javac -source 11 -target 11 -cp "$ANDROID_JAR" \
  -d build/obj $(find src -name '*.java') build/R.java
ls build/obj/com/lxsh/relay 2>/dev/null || { echo "javac failed"; exit 1; }

echo "==> [3/6] d8"
rm -rf build/dex; mkdir -p build/dex
java -cp "$JARS/d8.jar" com.android.tools.r8.D8 \
  --min-api 26 --lib "$ANDROID_JAR" --output build/dex \
  $(find build/obj -name '*.class')

echo "==> [4/6] inject dex"
cp build/dex/classes.dex build/classes.dex
cd build
aapt add base.apk classes.dex
cd ..
rm -f build/aligned.apk build/LXSH2-debug.apk

echo "==> [5/6] zipalign"
zipalign -f 4 build/base.apk build/aligned.apk

echo "==> [6/6] sign"
java -jar "$JARS/apksigner.jar" sign \
  --ks "$KS" --ks-pass pass:android --key-pass pass:android \
  --ks-key-alias androiddebugkey --min-sdk-version 26 \
  --out build/LXSH2-debug.apk build/aligned.apk

echo "==> 完成"
ls -lh build/LXSH2-debug.apk
