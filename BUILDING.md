# Building and distributing

The main build is Windows x64 (`x86_64-pc-windows-msvc`); the Android build is under
[Android](#android). Install Rust stable
and the Visual Studio Desktop development with C++ workload with the Windows SDK.
The prepared v1.1.1 build uses rustc/cargo 1.99.0. `Cargo.lock` pins dependency versions.
`build-release.ps1` applies the release build's source-path remapping so a local user
profile does not appear in compiled source-location strings. Run it from PowerShell:

```powershell
./build-release.ps1
```

```powershell
cargo test --locked --workspace
cargo build --locked --release -p jc-game
```

Run the resulting `target/release/jc-game.exe` from a writable
directory, or `cargo run --locked --release -p jc-game` from this repository. Runtime
game content must come from your own lawful image; it is not needed to compile or
run the synthetic unit tests. Bevy shaders in this repository are compiled into the
application; no original game assets are included in the build.

To distribute a binary, provide its exact Corresponding Source and dependencies as
required by GPLv3 and retain applicable notices. For this release, keep the Windows
ZIP, the Android APK, the matching complete source archive and SHA256SUMS.txt together. The source archive
includes vendored Cargo dependency source, Cargo.lock and the offline Cargo config.
After extracting the complete source archive, add `--offline` to the commands above
to build without downloading Cargo dependencies, or run `./build-release.ps1 -Offline`.
The archive's `SOURCE-REVISION.txt` names the release commit. The build reads it, so an
executable built from the complete source archive can play Arena Battle with the
released one (two builds must name the same version and commit to play together).
Rust, the C++ toolchain, Windows SDK
and ordinary system libraries are separate prerequisites.

## Android

The Android APK (arm64) is the same game built as a native library (`crates/jc-android`,
which compiles `crates/jc-game/src` as a `cdylib`) inside a small Gradle project
(`android/`). It needs, besides Rust: the Android SDK with an NDK r27 or later (Android
Studio's SDK Manager), a JDK 17 or later (Android Studio's own `jbr` is found by itself),
the Rust target and cargo-ndk:

```powershell
rustup target add aarch64-linux-android
cargo install cargo-ndk
./android/build-apk.ps1
```

The APK lands in `dist/JadeCocoon-android-arm64.apk`. Without further options it is
signed with your PC's Android debug key, enough to install it by hand.
`-Keystore path/to/keystore.properties` signs it with your own key (a properties file
with `storeFile`, `storePassword`, `keyAlias` and `keyPassword`; keep it out of every
repository). An APK signed with one key does not install over one signed with another.
`-Install` also installs it on a phone connected with USB debugging.

From the complete source archive, set `$env:CARGO_NET_OFFLINE = 'true'` first so cargo
uses the vendored dependencies. Gradle downloads the Gradle distribution, the Android
Gradle Plugin and the AndroidX libraries (Apache-2.0) from their public repositories.
The APK's version is the workspace version in `Cargo.toml`. `build-apk.ps1` remaps the
local user profile out of the library's source paths, as `build-release.ps1` does.

## Packaging

Use an explicit file list when packaging. Exclude local configuration, original game
content, saves, captures, caches, build caches and Git databases. Do not zip your
entire development directory. Retain the GPL licence, legal notice, dependency notices,
instructions and exact source for the executable you distribute.
