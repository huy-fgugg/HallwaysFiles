#!/usr/bin/env bash
# Builds android/app/build/outputs/apk/release/app-release.apk.
# Needs: ANDROID_NDK_HOME (or ANDROID_NDK_ROOT), the Android SDK, Java 17,
# Gradle, `rustup target add aarch64-linux-android` and `cargo install cargo-ndk`.
set -euo pipefail
cd "$(dirname "$0")/../.."
rm -rf android/app/src/main/jniLibs android/app/src/main/assets
(cd mobile && cargo ndk -t arm64-v8a --platform 28 --link-libcxx-shared -o ../android/app/src/main/jniLibs build --release)
cp -r assets android/app/src/main/assets
(cd android && gradle --no-daemon assembleRelease)
ls -l android/app/build/outputs/apk/release/
