# ZakoAI Mobile 0.7.7 — Build Artifacts

Built from `feature/mobile-agent` @ `fb8115a` on 2026-10-18.

## APK
- `ZakoAI-0.7.7-mobile-universal.apk` — arm64-v8a + x86_64, minSdk 24, targetSdk 28
- Debug-signed. Install with `adb install -r` or `pm install -r`

## Backend binaries
- `zakoai-proxy-0.7.7-aarch64-linux-android` — Android/Termux arm64
- `zakoai-proxy-0.7.7-x86_64-linux-android` — Android x86_64 emulator

## Verify
```sh
sha256sum -c SHA256SUMS.txt
```

> Debug signature only; not for production distribution.
