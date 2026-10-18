# Building and distributing

The supported build is Windows x64 (`x86_64-pc-windows-msvc`). Install Rust stable
and the Visual Studio Desktop development with C++ workload with the Windows SDK.
The prepared v1.0.1 build uses rustc/cargo 1.99.0. `Cargo.lock` pins dependency versions.
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
ZIP, matching complete source archive, and SHA256SUMS.txt together. The source archive
includes vendored Cargo dependency source, Cargo.lock and the offline Cargo config.
After extracting the complete source archive, add `--offline` to the commands above
to build without downloading Cargo dependencies, or run `./build-release.ps1 -Offline`.
Rust, the C++ toolchain, Windows SDK
and ordinary system libraries are separate prerequisites.

Use an explicit file list when packaging. Exclude local configuration, original game
content, saves, captures, caches, build caches and Git databases. Do not zip your
entire development directory. Retain the GPL licence, legal notice, dependency notices,
instructions and exact source for the executable you distribute.
