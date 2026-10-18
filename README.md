# Jade Cocoon on Bevy

A free, unofficial fan-made engine reimplementation for **Jade Cocoon: Story of the
Tamamayu** (PlayStation, European release, `SLES-02201`), written in Rust on
[Bevy](https://bevyengine.org). This is a personal, noncommercial project made out of
love for the game, for compatibility, learning and preservation.

**Original game files are not included. You must lawfully possess an original copy
of the supported game and have the right to make and use your own disc image.**
The application reads game content from that local image. It reads data tables from
the executable and interprets the disc's scene scripts; it does not execute the
original PlayStation native machine code. See [LEGAL.md](LEGAL.md).

## Version 1.0.1

The story plays through its ending, with battles, creature capture and merging, menus,
movies, music, voices, and local save/load. The optional second-round corridor is
implemented. Version 1.0.1 is a bug-fix update of 1.0.0: purifying gives the empty
cocoons back again, as in the original game. This is a fan implementation with known
differences and bugs, rather than a promise of complete accuracy. See
[HOW-TO-PLAY.md](HOW-TO-PLAY.md) and [release notes](RELEASE-NOTES.md).

## Play on Windows

Extract the entire Windows x64 release ZIP to a writable folder. Double-click
`play.bat`. On first start, choose your own European raw MODE2/2352 `.bin` disc image
when prompted. The path is stored locally in `jc.local.toml`; saves are stored in
`saves/`. No Rust installation is needed for the packaged Windows build.

Do not use a disc image downloaded from somebody else merely because you own a disc.
Only make and use an image where permitted by applicable law. This project does not
provide game files or links to obtain them.

## Build from source

Install Rust stable for `x86_64-pc-windows-msvc` and the Visual Studio **Desktop
development with C++** workload, including the Windows SDK. From this directory:

```powershell
cargo build --locked --release -p jc-game
cargo run --locked --release -p jc-game
```

The executable is `target/release/jc-game.exe`. Unit tests use synthetic inputs:

```powershell
cargo test --locked --workspace
```

The source release does not require or contain any original game files. Only running
the application requires your separately obtained lawful game copy. Build information
and third-party notices are in [BUILDING.md](BUILDING.md) and
[THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md).

## Licence and affiliation

Copyright (c) 2026 ufodriverr and contributors. Original project code and documentation
are licensed under [GNU GPL version 3 only](LICENSE) (`GPL-3.0-only`), without warranty
to the extent permitted by law. Dependencies retain their own licences.

The maintainer distributes this project for free and does not commercialise it.
GPLv3 nevertheless permits downstream commercial use of the covered project code;
the project's purpose does not add a noncommercial restriction to the licence.
GPLv3 grants no rights to the original game, its content or third-party trademarks.

This project is not affiliated with or endorsed by the game's developers, publishers
or other rights holders. The game title is used to identify compatibility. All rights
in the original game remain with their respective rights holders.

Rights holders: see the [contact and removal process](LEGAL.md#rights-holder-contact-and-removal).
The maintainer will promptly suspend affected published material under their control
on receipt of a credible rights-holder complaint, including the repository if necessary.
Please read [CONTRIBUTING.md](CONTRIBUTING.md) before submitting a change or report.
