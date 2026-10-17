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

## Version 1.1.1

The story plays through its ending, with battles, creature capture and merging, menus,
movies, music, voices, and local save/load. The optional second-round corridor is
implemented. Version 1.1 adds a large set of PC improvements and the two-player
**Arena Battle** over a network or the internet. Version 1.1.1 adds an **Android** build
for arm64 phones, with on-screen touch controls; the game is otherwise the same as
1.1.0. This is a fan implementation with known differences and bugs, rather than a
promise of complete accuracy. See [HOW-TO-PLAY.md](HOW-TO-PLAY.md) and the
[release notes](RELEASE-NOTES.md).

### What's new in 1.1.1

- **Android:** the same game as an APK for arm64 phones (Android 9 or later). The first
  start asks for your disc image with the system's file picker and copies it into the
  app's folder once. On-screen buttons sit beside the 4:3 picture: an analog stick, the
  four PlayStation buttons, the shoulders, Start, Select, the menu and fast forward; a tap
  elsewhere picks a menu row or confirms, like the PC's mouse. Gamepads work too.
- Saves are the same files on both: copy `saves` between the PC and the phone by USB.
- Arena Battle works between a phone and a PC when both run 1.1.1.

### What's new in 1.1

- **Arena Battle (two players):** the title's "Arena Battle Stage" is back as a fight
  between two players on two PCs. Each brings up to three minions from one of their
  own saves. Play on the same network, by IP address, or over the internet with a
  room code (no router setup). Nothing is ever saved by a match.
- **Screen:** fullscreen 16:9 by default, true widescreen fights, smooth motion between
  the game's 25 steps a second, and options for 4:3 screens, resolution, VSync, frame
  cap, integer scaling, an "authentic" PS1 look, anti-aliasing, scanlines/CRT and dither.
- **PC Settings:** a new page in the options (where Vibration was) with Display, Sound,
  Gameplay and Controls, including key and button rebinding. Kept in `settings.toml`.
- **Controls:** gamepads (Xbox layout, DualSense and others), the mouse in nearly every
  menu, typing your name on the keyboard, Enter confirms, Caps Lock walk, and on-screen
  button prompts that show your keys or your pad's buttons.
- **Pace:** hold Left Ctrl (pad: RT) to fast-forward, fight speed 1-3x, and hold Enter
  (pad: Start) to skip a scene you have seen before.
- **Saves:** F6 quick save, F9 quick load, autosaves (entering a forest, a chapter start,
  before a boss), and a save list with date, time and chapter.
- **Safety and sound:** closing the window and Ctrl+F5 ask before unsaved progress is
  lost; separate volumes; the game pauses when its window is in the background; logs
  and crash notes go to `logs\`.
- **Optional assists** (off by default): experience and money multipliers, creatures
  always come back, fewer encounters.

## Play on Windows

Extract the entire Windows x64 release ZIP to a writable folder. Double-click
`play.bat`. On first start, choose your own European raw MODE2/2352 `.bin` disc image
when prompted. The path is stored locally in `jc.local.toml`. The game starts
fullscreen; F11 or Alt+Enter switches to a window. Settings are kept in
`settings.toml`, saves in `saves\` and logs in `logs\`, all in the game folder. No Rust
installation is needed for the packaged Windows build.

Do not use a disc image downloaded from somebody else merely because you own a disc.
Only make and use an image where permitted by applicable law. This project does not
provide game files or links to obtain them.

## Play on Android

Copy the release's `.apk` to the phone and open it; Android asks once to allow installs
from that app (your file manager or browser). Copy your own `.bin` image to the phone as
well (Downloads is fine). On first start, tap **Choose disc image (.bin)** and pick it in
the system's file picker: it is copied once into the game folder and checked, and later
starts go straight to the game. The `.bin` you copied can then be deleted.

The game folder is `Android/data/io.github.ufodriverr.jadecocoon/files` on the phone's
storage, with `saves`, `settings.toml` and `logs` as on Windows; a PC reaches it over USB.
Uninstalling the app deletes that folder, saves included, so copy `saves` off first. The
controls are in [HOW-TO-PLAY.md](HOW-TO-PLAY.md#android).

Arena Battle connects your device to another player's device. Read
[LEGAL.md](LEGAL.md#network-and-privacy) for what that means for your privacy.

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
(including the Android APK) and third-party notices are in [BUILDING.md](BUILDING.md) and
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
