# Locus: status

Where the work stands against [plan.md](plan.md). The plan says what to do and [intent.md](intent.md) says why. This file only tracks progress, so update it when a stage moves. Last updated 2026-10-20.

## Now: stages 1 and 2, a first version to try

The near-term goal is a version the author can use day to day: working browser and Linux desktop apps for notes, todo, calendar, files, photos, movies, music, messages and contacts, plus the `locus` command line for everything, running on the clan server. The agent builds and tests it end to end, then the author tries it.

### 1. Foundation

- Done: a NixOS server with ZFS, the Urbit ship as a service, and `locus-gateway` serving the browser app, file bytes and the ship's API. The first server runs it, and the Obsidian vaults are imported as notebooks.
- Done 2026-10-20: one install procedure from a blank disk, [install.md](install.md). The steps are: the host template (`nix flake new -t …#server`), disko, `nixos-install`, then reboot. `locus-desk.service` installs the `%locus` desk by itself on first boot and updates it after each rebuild. It snapshots the pier first. On a new ship it suspends Tlon's `%groups` and clears its `%notes` and `%contacts`. `scripts/fresh-install-test.sh` runs the procedure in a VM from a stock NixOS minimal installer.
- Done 2026-10-20: moving data to a new identity. `locus backup export <dir>` and `locus backup import <dir>` copy notebooks, folders, notes, files with their bytes, photos, events and contacts. Import skips what is there, so it is safe to rerun. Note history, timestamps and messages are not carried over.
- Done 2026-10-20: off-machine copies. The pier and the Locus data export are on the laptop (`~/Backups/`) and on here-nor-there (`/data/Backups/surface-locus/`), checked by SHA-256.
- The server's comet was reset on 2026-10-20. A restore test had booted a copy of the old comet's pier online, and the network then refused the real ship ("double-boot detected"). The data was exported from an offline copy of the old pier and imported into a new comet, `~masned-savted-difted-locden--rovruc-sonsem-witper-wanzod`: 242 notes in 9 notebooks, 31 folders and 12 files. An export of the new ship matches the old one item for item. The old pier is kept in `rpool/urbit/pier@pre-reset-20261020`. The rollback docs used to say to roll the pier back and start it; they now explain why that loses the identity ([urbit.md](urbit.md#rollback-and-why-the-pier-must-not-go-back-in-time)).
- Open:
  - The server runs on a comet. The planet (the clan's root identity) comes later, and moves the data the same way.
  - The ship's Azimuth eth-watcher error ("range 100000 exceeds limit of 10000") is unfixed. Public Ethereum endpoints cap `eth_getLogs` below that range, so it needs an RPC provider with a higher limit, or a local node.

### 2. Core apps (in progress)

- Done and tested (unit tests, plus clicking through each app against real data with throwaway items):
  - Eight standard point-and-click apps. Each runs in its own browser tab (`/app/<app>`) and as its own desktop program (`locus-<app>`). Todo is a tab of the Calendar app, so it has no app of its own.
  - The `locus` CLI covers the same actions as the apps.
  - Each app has its own launcher and icon, and there is no combined Locus window. Files, Notes and Movies use the icons of COSMIC Files, Edit and Player; Photos, Music, Calendar, Contacts and Messages have icons drawn in the same style (`share/icons`). Todo opens from Calendar.
  - Accent colour: the desktop apps follow the XDG portal's accent and dark or light setting (libcosmic's `xdg-portal` feature), so they match GNOME as well as COSMIC. The browser app uses the CSS `AccentColor`, which Firefox maps to the desktop's accent; other browsers keep the default blue.
  - Notes render Obsidian style (2026-10-21): headings, bold, italic, strike, highlight, code, wiki and web links, tags, nested lists, task checkboxes that tick and save, quotes, callouts, code blocks, rules and tables. A click on the text opens the Markdown source at that line; Ctrl+E, Done or Escape goes back. The source is coloured as you type, and a toolbar and shortcuts (Ctrl+B, I, Shift+X, Shift+H, \`, 1-3, L) format the selection. Notes stay plain Markdown. Tested in the desktop app (render, tick, click to edit, Ctrl+B on and off, a wiki link to another notebook) and the browser app (render, click to edit, Ctrl+E back). Not checked: formatting a selection made by dragging the mouse.
  - Notes: folders, notebooks, rename, move, delete, revision history and restore, move between notebooks with a destination picker, and conflict copies. Moving a note between notebooks is tested in both apps (desktop and browser).
  - Todo: tasks are checkbox lines in notes. New tasks go to the notebook named "Tasks" if there is one, otherwise to the notebook being browsed.
  - Files: upload, folders, rename, move, delete. Folders can be renamed and moved (the `%files` desk has `dir.move`). A sidebar lists "All files" and the folder tree (top-level folders, the branch down to the open folder, and its subfolders). The Name, Size and Modified headers sort the listing; clicking the sorted one again reverses it, and Ascending and Descending set the direction. Folders always come first.
  - Photos: albums, viewer, rename, move, delete. A video in the library plays in the desktop viewer (tested 2026-10-20: frames advance, the time counts up, and the controls show).
  - Calendar and Contacts: create, edit, delete.
  - Movies and Music: views over Files. A movie is a video file at or below `/Movies`; a track is an audio file at or below `/Music` (in `/Music/<artist>/<album>/`, for example). Transport tested on 2026-10-20 in both apps:
    - Browser: play, pause (the position holds), seek while paused (lands and stays paused), resume from the seek point, volume, stop; for music also next, previous (restart past 3 s, previous track within 3 s) and moving on to the next track at the end.
    - Desktop: play, pause, seek while paused, seek while playing, resume, volume, stop; for music also next, previous and moving on at the end.
  - Messages: direct messages between ships, sent as `%messages` pokes over Ames. A message you send shows as sending until the other ship's agent has it, then as delivered. If the ship refuses it, it shows as failed and the text goes back into the draft. Messages you send to yourself are stored once. Conversations list newest first, and an open conversation clears its unread count.
  - Spam protection: a ship's incoming messages are refused when it is blocked (`locus messages block ~ship`, or the Block button in the conversation), and the 21st message from one ship within a minute is refused. A refused message stores nothing, and our own sends to a blocked ship still go. Deployed 2026-10-20.
  - Two-ship messages, checked on two fake ships (`~/exp/two-ship-test.sh`, 2026-10-20, all 12 checks pass): a message both ways, the sender's copy goes from pending to delivered, a send to a ship that is offline stays pending and then arrives when it is back, and junk pokes are refused without storing anything. The test logs in with the ship's code, because gall scries are cancelled on fake ships.
- Performance: every core operation measured in release is under 16.6 ms (slowest: opening a folder of 10,000 files, about 10 ms). The shells draw long lists as virtual rows: Notes, Files, Contacts, Photos, Movies, Music, Messages, and now Todo and the calendar agenda.
- Group conversations (built 2026-10-20, deployed to the new comet the same day):
  - A group has an id (owner, dash, count: `~zod-1`), a title, an owner and members. Writing to a group sends it to every other member; the message is delivered once all of them have it, and shows who is still waiting.
  - A member learns of a group from its first message, which carries the title and members. Later messages must come from a member, and so must the group's first message. The member list comes from the group's creator, so a creator can list members who never agreed; that is accepted for a family and friends clan, not for open groups.
  - A group cannot be blocked (block a member instead). There is no add, remove or leave yet, and no rename.
  - Checked: the messages Hoon suite (23 tests, four of them new), the Rust tests (core: 30 messages tests, all packages pass), the CLI (`locus messages group-new`, `send`, `show`, `read`, `ls`), two fake ships (create, send, reply, an offline member, read: 10 of 10), and the desktop and browser apps (create a group, send from the window and the browser, a reply shows the author).
- Fixed 2026-10-20:
  - Desktop scroll-to-selection drifted on long lists (commit `139d8ca`). Checked: 60 notes, the selection stays on screen.
  - Desktop seeks landed at the start of the movie (commit `d824319`). The player seeked to the previous keyframe; now it lands on the requested time. Checked in the desktop app, paused and playing.
- Deployed to the Surface server on 2026-10-20: the %locus desk on the new comet, with the messages fixes, spam protection and group conversations. The desktop programs, the gateway and the `locus` CLI are rebuilt from the same commit. All seven agents serve.
- Next:
  1. The author tries the whole set. This stage is done when they use it as their only system for notes, files and calendar/tasks.

## Later stages

Stages 3–7 (agents and feedback, the clan, clients, heavy compute, other clans) have not started. A command plane with members, named functions, approvals and an audit log already exists from earlier work. It is the starting point for stage 3.
