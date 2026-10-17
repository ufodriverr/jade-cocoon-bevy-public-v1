# Version 1.1.1

Prepared 2026-10-17. The Android update: the same game as 1.1.0, now also as an APK for
arm64 Android phones.

## Android

- An APK for arm64 phones (Android 9 or later; developed on a Galaxy S24 with Android 16).
- The first start asks for your own disc image with the system's file picker, copies it
  into the app's folder once and checks it; if the game cannot read it, the screen comes
  back with the reason. Later starts go straight to the game.
- **Touch controls**, drawn beside the 4:3 picture: an analog stick (a light push walks,
  a firm one runs), cross, circle, triangle and square (a thumb can slide between them),
  L1, L2, R1, R2, Select, Start, a menu button and a fast-forward toggle. A tap anywhere
  else works like the PC's mouse: tap a menu row to pick it, or confirm in a room. The
  back gesture is triangle. A short vibration ticks under the thumb.
- Gamepads work as on Windows; the on-screen buttons hide while one is used.
- The help lines show the PlayStation signs on a phone.
- The game folder is `Android/data/io.github.ufodriverr.jadecocoon/files`: saves,
  settings and logs are the same files as on Windows, so a save can be copied between
  the PC and the phone over USB. Uninstalling the app deletes that folder.
- The game pauses while the app is in the background and the screen stays on while it
  plays. Arena Battle works between a phone and a PC that both run 1.1.1; "Paste" takes a
  room code from the phone's clipboard.
- The APK carries the licence and the notices (`assets/legal`) and names the source on
  its first screen.

## Notes for 1.1.1

- Windows: the game is unchanged from 1.1.0; the new Windows ZIP is built from the same
  commit as the APK, so the two can play Arena Battle together (two builds of different
  commits refuse each other). `--touch-controls` shows the touch buttons on a PC.
- The APK is signed with the project's own release key. A test build signed with another
  key does not install over it: copy `saves` off the phone, uninstall, then install.
- The Android build adds no Rust dependencies; the APK includes the AndroidX libraries
  and GameActivity (Apache-2.0), listed in [THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md).
- Tried on a Galaxy S24 and the Android emulator only. Older phones and phones without
  Vulkan are untested.

# Version 1.1.0

Second public release, prepared 2026-10-16. Free, unofficial, noncommercial fan project.
One big update: PC quality-of-life and modernization, and the two-player Arena Battle.

## Arena Battle (two players)

- The title's "Arena Battle Stage" is back as a fight between two players on two PCs,
  as in the original game's own two-player mode: minions only, one on the field per
  side, up to three a side, one of the story's 39 fight places and the arena's music.
- Pick a save, up to three of its minions in fight order, and a name. One player hosts,
  the other joins.
- Connect over the internet with a **room code** (no router setup; direct, or through
  an encrypted relay when needed), on the **same network** (hosts are listed), or **by
  IP address** (the host's router may forward the port by UPnP).
- The host sets the terms: items from your save or original rules without items,
  changing minions on or off, and the place.
- Each match starts with a visible dice roll for who goes first, then the story's fight
  rules: Attack, Defend, Special, Magic, Switch and Use. Give up, win tally and Rematch.
- Nothing is ever saved by a match. Both games compute the same fight from a shared seed
  and check that they agree every turn; different versions or discs are refused.
- See [LEGAL.md](LEGAL.md#network-and-privacy) for what network play means for privacy.

## PC quality of life and modernization

- **Display:** fullscreen 16:9 by default; every screen scales the same way, with
  blurred sides, black bars, stretch or zoom for 4:3 pictures. True widescreen fights.
  Smooth motion between the game's 25 steps a second. Resolution list, VSync, frame cap,
  integer scaling, an "authentic" PS1 look, anti-aliasing, scanlines/CRT and dither.
- **PC Settings:** a new page in the options (in place of Vibration) with Display,
  Sound, Gameplay and Controls, including key and button rebinding. Everything is kept
  in `settings.toml` beside the exe; saves keep the game-feel settings too.
- **Controls:** gamepads (Xbox layout, analog stick), Enter confirms, Caps Lock toggles
  walk, the mouse in nearly every menu (enemy targets included), typing your name on
  the keyboard, on-screen button prompts that follow your keyboard or pad, and a button
  hint over the hero when confirm would do something.
- **Pace:** hold Left Ctrl (pad: RT) to fast-forward; fight speed 1-3x; hold Enter (pad:
  Start) for a second to skip a scene you have seen before.
- **Saves:** F6 quick save and F9 quick load; autosave on entering a forest, at a chapter
  start and before a boss; the save list shows date, time and chapter.
- **Safety:** the close button and Ctrl+F5 (restart; F5 alone no longer restarts) ask
  before unsaved progress is lost. Logs and crash notes go to `logs\`.
- **Sound:** master, music, effects, voices and movies volumes; the game pauses when its
  window is in the background.
- **Assists** (off by default): experience and money multipliers, creatures always come
  back, fewer encounters.

## Notes

- Windows x64 build; European PlayStation disc `SLES-02201`, raw MODE2/2352 `.bin`.
- No original game content, disc image, BIOS, personal configuration or saves included.
- GPL-3.0-only source with dependency notices and a matching complete source archive.
  The networking library and its dependencies are listed in
  [THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md).
- The executable is larger than 1.0.0's because of the networking library.
- Known limits of Arena Battle: the greeting scene in the arena's room is not played
  (the lobby replaces it), and the commentator's voice lines are not included.
- Version 1.1.0 identifies this release; it is not a claim of complete accuracy or that
  every route has been tested. Known difference: merged creatures retain the first
  creature's bone lengths. Further compatibility and visual issues may remain.

See [HOW-TO-PLAY.md](HOW-TO-PLAY.md) for controls, settings, Arena Battle and reporting
issues. See [LEGAL.md](LEGAL.md) for the lawful-copy requirement, network and privacy,
and rights-holder contact.

# Version 1.0.0

First public release, prepared 2026-10-14. Free, unofficial, noncommercial fan project.

- Windows x64 build; European PlayStation disc `SLES-02201`, raw MODE2/2352 `.bin`.
- Story progression through the ending, battles, capture, creature management and
  merging, menus, movies, music and voices, and local saving/loading.
- Optional second-round corridor, speed controls and restart.
- No original game content, disc image, BIOS, personal configuration or saves included.
- GPL-3.0-only source with dependency notices and a matching source archive.

Version 1.0.0 identifies this release; it is not a claim of complete accuracy or that
every route has been tested. Known difference: merged creatures retain the first
creature's bone lengths. Further compatibility and visual issues may remain.
