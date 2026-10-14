# How to play version 1.0.0

This Windows x64 fan implementation supports the European PlayStation release of
Jade Cocoon: Story of the Tamamayu (`SLES-02201`). You must lawfully possess an original
copy and have the right to make and use your own image. Game files are not supplied.
See [LEGAL.md](LEGAL.md).

## Start

Extract the entire Windows release ZIP to a writable folder. Double-click `play.bat`;
choose your own raw MODE2/2352 `.bin` image when asked. The path is saved in
`jc.local.toml`. If the image is moved or cannot be read, the next start asks again.
Cancel closes the application. Other regions and image layouts are unsupported.

For a source checkout, install the prerequisites in [BUILDING.md](BUILDING.md), then
run `cargo run --locked --release -p jc-game` from the repository root.

## Controls

| Keyboard | PlayStation button / action |
|---|---|
| W A S D / arrow keys | Directions |
| Shift | Run while walking |
| K / X | Cross: confirm, talk, inspect, open a door while facing it |
| L | Circle: describe in menus |
| I / Backspace | Triangle: cancel / room menu |
| J | Square |
| Enter | Start / skip a skippable movie |
| Tab | Select |
| Q / E | R1 / R2 |
| U / O | L1 / L2 |
| F2 | Cycle 2x, 4x, 8x and normal speed |
| F1 | Restore normal speed |
| F5 | Restart the game |

## Save and load

Local saves are in `saves/` beside the executable when started with `play.bat`.
The PC save screen has fifteen slots. Continue on the title opens the save list.
In a room, face the save light and press confirm to open saving/loading. Deleting
a save through this menu moves it to `saves/deleted/`; back up saves yourself before
upgrading or experimenting. Original PlayStation memory-card import is not promised.

## Included and known differences

The story through its ending, battles, capture, creature management and merging,
menus, movies, music, voices and save/load are implemented. The optional second-round
corridor is available. Not every path and combination has been tested, and full
accuracy is not guaranteed. A merged creature currently retains the first creature's
bone lengths, so proportions can differ. See [RELEASE-NOTES.md](RELEASE-NOTES.md).

## Troubleshooting

Use the supported European raw `.bin` image and keep the extracted release folder
writable. Running `play.bat` keeps the console available after a launch failure.
Report your Windows version, the release version, what you did, and a short relevant
error message. Remove private paths from logs and do not attach disc files, extracted
content or game-content dumps. Preserve your own saves before trying another build.
