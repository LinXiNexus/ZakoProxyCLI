# LXSH Mobile 1.2.0 — Build Artifacts

Built from `feature/mobile-agent` on 2026-10-18.

## APK
- `LXSH-1.2.0-universal.apk` — arm64-v8a + x86_64, minSdk 24, targetSdk 28
- Debug-signed. Install with `adb install -r` or `pm install -r`

## Backend binaries
- `lxshai-proxy-1.2.0-aarch64-linux-android` — Android/Termux arm64
- `lxshai-proxy-1.2.0-x86_64-linux-android` — Android x86_64 emulator

## Verify
```sh
sha256sum -c SHA256SUMS.txt
```

> Debug signature only; not for production distribution.
