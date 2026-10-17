# Legal notice

## Project and game content

Jade Cocoon on Bevy is a free, unofficial fan-made engine reimplementation for
**Jade Cocoon: Story of the Tamamayu**, targeting the European PlayStation release
`SLES-02201`. The maintainer develops and distributes it as a personal,
noncommercial project for compatibility, learning and preservation.

The project distributes its implementation and documentation. Original game disc
images, executables, PlayStation BIOS files, graphics, models, animations, fonts,
dialogue, music, voices and movies are not included.

The application reads and decodes content from the user's local disc image. It also
reads data tables from the game executable and interprets scene scripts from the disc.
It does not execute the original PlayStation native machine code. Locally produced
screenshots, extracted content and other game-derived outputs are not licensed by
this project and are excluded from its distributions.

On Android, the app copies the disc image you choose into its own folder on your
device (`Android/data/io.github.ufodriverr.jadecocoon/files/disc.bin`) so the game can
read it. The copy never leaves your device through the app and is deleted with the app.

## A lawful original copy is required

You must lawfully possess an original copy of the supported game and have the right
to use it. Supply your own disc image, made and used only as permitted by applicable
law and any applicable licence. Possession of an original disc does not by itself
authorise downloading someone else's image, sharing a backup or distributing assets.

This project supplies no game downloads, BIOS files, decryption keys or piracy links.
Do not upload game files or game-content dumps to the repository, issues, pull
requests, releases or CI artifacts. Report bugs in your own words with a minimal
log that contains no game-content dumps or private filesystem paths.

## Ownership and affiliation

All rights in the original game and its content remain with their respective rights
holders. The game title and other third-party names and marks identify compatibility
and their respective owners. No rights in those names or marks are claimed here.

This project is not affiliated with, sponsored by or endorsed by Genki, Crave,
Ubisoft, Sony or any other rights holder. This notice does not purport to identify
the current ownership of every component of the game.

## Licence scope

Copyright (c) 2026 ufodriverr and contributors. The project's original code and
documentation are licensed under [GNU GPL version 3 only](LICENSE)
(`GPL-3.0-only`), subject to separately identified third-party licences. The work
is provided without warranty, to the extent permitted by applicable law.

GPLv3 permits use, modification and redistribution, including commercial use.
The maintainer's noncommercial purpose is not a restriction on recipients' GPL
rights. Preserve required notices and provide Corresponding Source when conveying
covered binaries, as required by GPLv3. Distributed covered modifications must
comply with GPLv3. Read the complete licence for its conditions and exceptions.

Dependencies retain their own licences; see [THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md).
No licence to the original game's code, content or trademarks is granted. The lawful
game-copy requirement concerns third-party game content and does not amend GPLv3
for the project's own work.

## Rights-holder contact and removal

If you hold affected rights or are authorised to act for a rights holder, open an
issue in the repository hosting this notice, titled **Rights-holder contact**, when
Issues are available. Identify the work, affected paths or URLs, your relationship
to the rights holder and the action requested. Do not post identity documents,
private addresses, confidential correspondence or copies of game files publicly.

On receipt of a credible rights-holder complaint, the maintainer will promptly
suspend public access to affected material under their control while addressing
the concern, including removing the repository if necessary. If the complaint
concerns the whole project, the maintainer will promptly suspend the repository's
public availability. The maintainer cannot guarantee deletion of independent forks
or copies held by others.

You may also use [GitHub's formal content-removal channels](https://docs.github.com/en/site-policy/content-removal-policies/submitting-content-removal-requests)
without contacting the maintainer first, including when Issues are unavailable.
This voluntary process does not replace those channels or limit anyone's rights.

## Network and privacy

The game uses the network only for **Arena Battle**, the two-player fight, and only
after you choose to host or join a match. The rest of the game works offline.

- **The other player sees your IP address.** Arena Battle connects your PC directly to
  the other player's PC whenever it can. Like any direct connection, this lets the other
  player's computer see your IP address, which can reveal your rough location and your
  internet provider. Play only with people you trust with that. The name you choose for
  the match is shown to the other player.
- **Room-code play may use n0's public relay servers.** Room-code play uses the
  [iroh](https://www.iroh.computer) library. When the two PCs cannot connect directly, the
  connection passes through a public relay server run by n0, a third party that is not
  part of this project. Relays only see encrypted traffic, not the contents of the fight.
  Like any server, they see the IP addresses that connect to them. While you host, your
  game also registers which relay it uses (not your IP address) with n0's public lookup
  service, under a key made from the room code, so your friend's game can find it. Anyone
  who has the room code can try to join, so give it only to the friend you play with.
  Every hosting makes a new code.
- **LAN discovery broadcasts on your local network.** When you look for hosts on the same
  network, your game sends a short "who hosts?" message to every device on the local
  network. A host answers with its player name, game version and port, so other devices
  on that network can see that a match is being hosted.
- **Hosting by address may open a router port.** When you host by address, the game asks
  your router (UPnP) to forward TCP port 47820 to your PC so a friend outside your network
  can join. The forward lasts for the hosting session (two hours at most) and the game
  asks the router to remove it when hosting stops. While it is open, anyone on the
  internet can reach that port. In room-code play the networking library may also ask
  the router for a port mapping (UPnP, PCP or NAT-PMP) to help the direct connection.
- **No accounts, no telemetry.** There are no accounts or sign-ins. The project does not
  collect, send or sell usage data, and it runs no servers of its own. Your settings,
  saves and logs stay on your PC or phone.

Windows may ask whether `jc-game` may use the network the first time you host or join.
Allowing it for **Private networks** is enough for play at home.

The Android app declares the permissions Arena Battle needs (internet access, the
network and Wi-Fi state, and Wi-Fi multicast for finding hosts on the same network) and
vibration for the on-screen buttons. Android grants these at install without asking; the
app asks for no other permission, and reads only the one file you pick.

## Scope of this notice

This notice states project policy; it is not authorisation from the original game's
rights holders or a guarantee of legal clearance. A free fan-project purpose does
not itself create an exemption. Removing material does not automatically settle
claims about earlier activity. GPLv3 does not require independent forks to disappear
when the maintainer removes their own repository.
