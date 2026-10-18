# How to play version 1.1.2

This fan implementation for Windows x64 and Android (arm64) supports the European
PlayStation release of Jade Cocoon: Story of the Tamamayu (`SLES-02201`). You must
lawfully possess an original copy and have the right to make and use your own image.
Game files are not supplied. On a phone, start with [Android](#android).
See [LEGAL.md](LEGAL.md).

## Start

Extract the entire Windows release ZIP to a writable folder. Double-click `play.bat`;
choose your own raw MODE2/2352 `.bin` image when asked. The path is saved in
`jc.local.toml`. If the image is moved or cannot be read, the next start asks again.
Cancel closes the application. Other regions and image layouts are unsupported.

The game starts fullscreen (borderless, 16:9 first). **F11** or **Alt+Enter** switches
between fullscreen and a window. Closing the window in a game with unsaved progress
asks first.

For a source checkout, install the prerequisites in [BUILDING.md](BUILDING.md), then
run `cargo run --locked --release -p jc-game` from the repository root.

## Controls

The keyboard, the mouse and a gamepad all work at the same time. The gamepad column uses
an Xbox pad's names; a DualSense or another pad has the same places (cross is A,
circle B, square X, triangle Y).

| Action | Keyboard | Mouse | Gamepad |
|---|---|---|---|
| Directions | W A S D / arrow keys | wheel: up and down in menus | d-pad, left stick |
| Cross: confirm, talk, inspect, open a door you face | Space, F, Enter (also K, X) | left button | A |
| Triangle: cancel, back out of a menu | Esc, Backspace (also I) | right button | B |
| Circle: describe in a menu | R (also L) | middle button | Y |
| Menu over a room | I, Tab | | Y opens it, B backs out |
| Square | C (also J) | | X |
| Start: skip a movie, the title | Enter | | Start (Menu) |
| Select | Tab | | Back (View) |
| L1 / R1 | Q / E (also U for L1) | back / forward button | LB / RB |
| L2 / R2 | Z / V (also O for L2) | | LT / RT (RT is R2 in menus only) |
| Walk instead of run (held) | Shift | | |
| Walk until pressed again | Caps Lock | | left stick button |
| Fast forward (held) | Left Ctrl | | RT (outside menus) |
| Skip a scene you have seen (hold 1 s) | Enter | | Start |
| Quick save / quick load | F6 / F9 | | |
| Cycle 2x, 4x, 8x and normal speed | F2 | | |
| Normal speed | F1 | | |
| Restart the game (asks first if unsaved) | Ctrl+F5 | | |
| Fullscreen / window | F11, Alt+Enter | | |

**Walking.** The directions go the way the screen shows and the hero turns by himself.
The directions run; hold Shift to walk, or press Caps Lock to walk until you press it
again. A light push of the gamepad's stick walks and a firm one runs. "Tank" movement
(up and down walk forward and back, the sides turn) is on the Controls page of the PC
Settings.

**Enter** confirms in every menu and is still Start on the title and in a movie.

**Typing a name.** On the name entry you can type the name on the keyboard: letters,
digits, space and `, . ? ! ~ -`. Backspace removes the last letter and Enter takes the
name.

**The mouse.** Point at a menu row and the cursor goes there; a left click picks it.
This works in nearly every menu: the title, the save list, the options, the place map,
the menu over a room, the shop and the blacksmith, Mahbu's screen, the name entry and a
fight's command menu, including choosing a target. The wheel moves one row per notch.

**Button prompts.** The help lines at the bottom of the menus show the keys you play
with, or your pad's buttons as soon as you use a gamepad. When confirm would do
something where you stand (talk, a door, a save point, a switch), a small button hint
floats over the hero. Both can be changed on the Gameplay page of the PC Settings.

**Fast forward.** Hold Left Ctrl (pad: RT outside menus) and the game runs 4 times as
fast. A marker in the top right corner shows it. Movies keep their own pace, and voices
that would play are skipped so nothing sounds sped up.

**Skipping a scene.** While a scene runs, hold Enter (pad: Start) for a second. A ring
fills up, then the rest of the scene plays very fast behind a black screen, with the
same result as watching it. It stops by itself at the end of the scene, at a question
(never answered for you), a fight, a shop, a menu or a movie. By default only scenes
you have already seen to the end can be skipped; "Skip scenes" on the Gameplay page
changes this to Always or Off.

**Quick save and autosave.** **F6** saves into a quick save of its own while you walk
freely in a room ("Can't save now" otherwise), and **F9** loads it. The game also
saves by itself into three autosaves in turn: when you walk into a forest, when a
chapter starts, and before a boss or master fight. Autosave can be turned off on the
Gameplay page.

## PC Settings

Open **Options** (on the title, or the last row of the menu over a room). **PC
Settings** is the fourth row, where the original has Vibration. It has four pages: Q
and E (LB and RB) turn them. Left and right change a value; the mouse works too. Every
change is saved at once in `settings.toml`.

- **Display:** display mode (borderless, fullscreen, windowed), resolution, VSync, frame
  cap, smooth motion, how 4:3 screens fill a wide monitor (blurred sides, black bars,
  stretch, zoom), integer scaling, wide fights, render (sharp or authentic 320x240),
  anti-aliasing, scanlines/CRT filter, dither, and pause or mute while the window is in
  the background. A new display mode or resolution asks "Keep these settings?" and goes
  back by itself after ten seconds.
- **Sound:** master, music, effects, voices and movies volumes.
- **Gameplay:** fight speed (1-3x), fast forward speed, skip scenes, autosave, and the
  optional assists: experience and money multipliers, creatures always come back, fewer
  encounters (all off by default). Also button prompts and the interaction hint.
- **Controls:** movement (modern or tank), always run, Caps Lock walks, and every action
  with its key and gamepad button. Confirm on an action and press the new key or button
  to rebind it. "Reset to defaults" puts everything back. F1, F2, F5, F6, F9 and F11
  cannot be taken.

The volumes, pace, assists and prompts are also stored with each save and come back
when you load it.

## Files in the game folder

The game folder is the folder of `jc-game.exe`. It holds:

- `jc.local.toml`: the path of your disc image.
- `settings.toml`: your settings. You can edit it by hand while the game is closed; a
  broken file is moved to `settings.broken.toml` and the defaults are used.
- `saves\`: the fifteen save places (`card1\`), the quick save (`quick\`), the three
  autosaves (`auto\`) and deleted saves (`deleted\`). Each save has a small
  `.pc.toml` file beside it with its date, chapter and game-feel settings.
- `seen.toml`: the scenes you have seen, for skipping. Delete it and every scene is
  new again.
- `logs\`: `jc-game.log` for this start, `jc-game.prev.log` for the one before, and a
  `crash-<date>_<time>.txt` note if the game stops with an error.

## Android

The Android build (an arm64 phone, Android 9 or later) is the same game. Install the
release's `.apk`, copy your own `.bin` image to the phone, start **Jade Cocoon** and tap
**Choose disc image (.bin)**: the system's file picker opens, and the chosen image is
copied once (about 650 MB) into the game folder and checked. Later starts go straight
to the game. If the game cannot read the image, this screen comes back with the reason.

Buttons are drawn on the screen, beside the 4:3 picture on a wide phone:

| On screen | The pad |
|---|---|
| Stick, bottom left: put the thumb down anywhere there | Directions: a light push walks, a firm one runs; directions in menus |
| Cross, circle, triangle, square, bottom right | The same buttons (a thumb can slide from one to the next) |
| L1, L2, R1, R2, top corners | The shoulder buttons |
| SEL, START | Select and Start (hold START to skip a scene you have seen) |
| MENU | The menu over a room, and back out of it |
| `>>` | Fast forward on, and off with the next tap |
| A tap anywhere else | The PC's left mouse button: tap a menu row to pick it, a tap in a room is the cross |
| The phone's back gesture | Triangle |

A Bluetooth or USB gamepad works as on the PC; the on-screen buttons hide while it is
used and come back with the next touch. The help lines show the PlayStation signs
(`[prompts] style` in `settings.toml` changes it). The game pauses while the app is in
the background and goes on where it was when you come back.

The game folder is `Android/data/io.github.ufodriverr.jadecocoon/files`, with the same
files as on Windows plus the copy of the disc (`disc.bin`). Connect the phone to a PC by
USB (file transfer) to copy `saves` both ways: saves are the same files on both.
Uninstalling the app deletes this folder, saves included. The PC Settings' display
page has no effect on a phone, whose screen Android sizes.

In Arena Battle, "Paste" takes a room code from the phone's clipboard, and an address is
typed on the keypad under its field. A phone and a PC play together when both run the
same version.

## Save and load

The PC save screen has fifteen places. Continue on the title opens the save list; it
also shows the quick save and the three autosaves at the top, and the date, time and
chapter of the save under the cursor. In a room, face the save light and press confirm
to save or load. Deleting a save moves it to `saves\deleted\`. Back up your saves
yourself before upgrading or experimenting. Saves from version 1.0.0 load in 1.1.0, 1.1.1 and 1.1.2.
Original PlayStation memory-card import is not promised.

## Arena Battle (two players)

The original's **Arena Battle Stage** is back as a fight between two players on two
PCs. Each player brings up to three minions and the items of one of their own saves.
The fight uses the story's rules, turn by turn. Nothing is ever saved: every minion
starts each match fully healed, and items are restocked for each match.

Both players need **the same game version and the same disc**. A different version or
disc is refused with a message. Read [LEGAL.md](LEGAL.md#network-and-privacy) first:
the other player can see your IP address.

**Starting.** On the title pick the third row, **Arena Battle Stage**. Both players do
the same on their own PC:

1. **Your team:** pick one of your saves (read only), then up to three of its minions in
   fight order. **Shuffle** mixes the order and **Done** goes on.
2. **Your name:** the name the other player sees.
3. **Host or join:** one player picks **Host a match**, the other **Join a match**, then
   how to connect (below).
4. **The terms:** the host sets **Rules** (*Save items*, or *Original* with no items),
   **Changing** minions on or off, and the **Place** (random, or one of the story's 39
   fight places), then **Ready**. The other player answers **Ready** or **Leave**.

**Ways to connect:**

- **Online (room code)**, the easiest way over the internet, with no router setup. The
  host picks **Online (room code)** and **Copy the code**, then sends the code (twelve
  letters such as `k3fm-q8xa-7wpe`) to the friend in any chat. The friend picks
  **Online (room code)**, **Paste** (or Ctrl+V, or types it) and **Join**. The terms
  screen says whether the connection is **direct** or goes **through a relay**; both
  work well for this turn-based fight. A wrong or old code says "No host found for this
  code" after about 20 seconds. Every hosting makes a new code.
- **Same network:** the host picks **Same network or address**; the friend picks **Same
  network** and chooses the host from the list.
- **By address:** the friend types the host's IP address, for example `192.168.1.20` or
  `203.0.113.7:47820` (IPv6 works too). The host's screen lists its addresses. Matches
  use TCP port **47820**; finding hosts on the same network uses UDP port **47821**.
  When the host's router allows UPnP, the host's screen also shows "Your internet
  address" for a friend outside the network. Otherwise forward TCP 47820 on the router,
  or use a virtual LAN such as Tailscale or ZeroTier, or use a room code.

The first time you host or join, Windows may ask whether `jc-game` may use the network.
Allow it for **Private networks**. If it was refused, room codes still work through a
relay, but hosting by address does not.

**A match:**

- **The dice:** each match starts with one die each. The higher roll goes first; a draw
  rolls again.
- **Your turn:** Attack, Defend, Special, Magic, **Switch** (another of your living
  minions takes its place) and **Use** (an item from your save on one of your own
  minions). Keys, pad and mouse work as in the story's fights.
- **Give up:** cancel on the first level of the menu, then Yes.
- **The end:** the winner and the tally of the series, then **Rematch?**. A No returns
  both players to the title.
- If the connection is lost (no answer for about ten seconds), the other player leaves,
  or the two games ever disagree about the fight, a message says so and the match ends.
  Closing the window during a match counts as a loss. F6, F9 and autosave do nothing in
  the arena.

The lobby keeps your name, the last address you joined and the port in `settings.toml`
under `[versus]`.

## Included and known differences

The story through its ending, battles, capture, creature management and merging,
menus, movies, music, voices and save/load are implemented. The optional second-round
corridor is available. Not every path and combination has been tested, and full
accuracy is not guaranteed. A merged creature currently retains the first creature's
bone lengths, so proportions can differ. In Arena Battle the greeting scene in the
arena's room is not played (the lobby replaces it), and the commentator's voice lines
are not included. See [RELEASE-NOTES.md](RELEASE-NOTES.md).

## Troubleshooting

Use the supported European raw `.bin` image and keep the extracted release folder
writable. If the game stops with an error, look in `logs\` for the crash note.
Report your Windows version, the release version, what you did, and a short relevant
error message. Remove private paths from logs and do not attach disc files, extracted
content or game-content dumps. Preserve your own saves before trying another build.
