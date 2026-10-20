# Locus: status

Where the work stands against [plan.md](plan.md). The plan says what to do and [intent.md](intent.md) says why. This file only tracks progress, so update it when a stage moves. Last updated 2026-10-20.

## Now: stages 1 and 2, a first version to try

The near-term goal is a version the author can use day to day: working browser and Linux desktop apps for notes, todo, calendar, files, photos, movies, music, messages and contacts, plus the `locus` command line for everything, running on the clan server. The agent builds and tests it end to end, then the author tries it.

### 1. Foundation (mostly done)

- Done: a NixOS server with ZFS, the Urbit ship as a service, and `locus-gateway` serving the browser app, file bytes and the ship's API. The first server runs it, and the Obsidian vaults are imported as notebooks.
- Done 2026-10-20: an off-machine copy of the ship's pier. A ZFS stream of `rpool/urbit/pier` is on the laptop (`~/Backups/surface-pier/`), checked by SHA-256 on both ends. The stream itself was not restored from the laptop; the same snapshot was restored locally and booted with networking off, and its data matched the live ship.
- Open:
  - The ship is down as of 2026-10-20. A restore test booted a copy of the pier with networking on, and the network recorded that boot as the latest version, so the real ship now refuses to start ("double-boot detected"). The only remedy the docs name is a factory reset, which clears the event log and gives the comet a new identity. This needs the administrator's decision. The pier is kept in `rpool/urbit/pier` and `rpool/urbit/pier-restore-20261019`.
  - The server runs on a comet. The planet (the clan's root identity) comes later.
  - The ship's Azimuth eth-watcher error ("range 100000 exceeds limit of 10000") is unfixed. Public Ethereum endpoints cap `eth_getLogs` below that range, so it needs an RPC provider with a higher limit, or a local node.
  - Nobody has yet tried the "fresh machine through one documented procedure" check from the plan.

### 2. Core apps (in progress)

- Done and tested (unit tests, plus clicking through each app against real data with throwaway items):
  - Nine standard point-and-click apps. Each runs in its own browser tab (`/app/<app>`) and as its own desktop program (`locus-<app>`). Todo is a tab of the Calendar app, so it has no app of its own.
  - The `locus` CLI covers the same actions as the apps.
  - Notes: folders, notebooks, rename, move, delete, revision history and restore, move between notebooks with a destination picker, and conflict copies. Moving a note between notebooks is tested in both apps (desktop and browser).
  - Todo: tasks are checkbox lines in notes. New tasks go to the notebook named "Tasks" if there is one, otherwise to the notebook being browsed.
  - Files: upload, folders, rename, move, delete. Folders can be renamed and moved (the `%files` desk has `dir.move`).
  - Photos: albums, viewer, rename, move, delete. A video in the library plays in the desktop viewer (tested 2026-10-20: frames advance, the time counts up, and the controls show).
  - Calendar and Contacts: create, edit, delete.
  - Movies and Music: views over Files. A movie is a video file at or below `/Movies`; a track is an audio file at or below `/Music` (in `/Music/<artist>/<album>/`, for example). Transport tested on 2026-10-20 in both apps:
    - Browser: play, pause (the position holds), seek while paused (lands and stays paused), resume from the seek point, volume, stop; for music also next, previous (restart past 3 s, previous track within 3 s) and moving on to the next track at the end.
    - Desktop: play, pause, seek while paused, seek while playing, resume, volume, stop; for music also next, previous and moving on at the end.
  - Messages: direct messages between ships, sent as `%messages` pokes over Ames. A message you send shows as sending until the other ship's agent has it, then as delivered. If the ship refuses it, it shows as failed and the text goes back into the draft. Messages you send to yourself are stored once. Conversations list newest first, and an open conversation clears its unread count.
  - Spam protection: a ship's incoming messages are refused when it is blocked (`locus messages block ~ship`, or the Block button in the conversation), and the 21st message from one ship within a minute is refused. A refused message stores nothing, and our own sends to a blocked ship still go. Deployed 2026-10-20.
  - Two-ship messages, checked on two fake ships (`~/exp/two-ship-test.sh`, 2026-10-20, all 12 checks pass): a message both ways, the sender's copy goes from pending to delivered, a send to a ship that is offline stays pending and then arrives when it is back, and junk pokes are refused without storing anything. The test logs in with the ship's code, because gall scries are cancelled on fake ships.
- Performance: every core operation measured in release is under 16.6 ms (slowest: opening a folder of 10,000 files, about 10 ms). The shells draw long lists as virtual rows: Notes, Files, Contacts, Photos, Movies, Music, Messages, and now Todo and the calendar agenda.
- Group conversations (built 2026-10-20, not deployed; the server is down):
  - A group has an id (owner, dash, count: `~zod-1`), a title, an owner and members. Writing to a group sends it to every other member; the message is delivered once all of them have it, and shows who is still waiting.
  - A member learns of a group from its first message, which carries the title and members. Later messages must come from a member, and so must the group's first message. The member list comes from the group's creator, so a creator can list members who never agreed; that is accepted for a family and friends clan, not for open groups.
  - A group cannot be blocked (block a member instead). There is no add, remove or leave yet, and no rename.
  - Checked: the messages Hoon suite (23 tests, four of them new), the Rust tests (core: 30 messages tests, all packages pass), the CLI (`locus messages group-new`, `send`, `show`, `read`, `ls`), two fake ships (create, send, reply, an offline member, read: 10 of 10), and the desktop and browser apps (create a group, send from the window and the browser, a reply shows the author).
- Fixed 2026-10-20:
  - Desktop scroll-to-selection drifted on long lists (commit `139d8ca`). Checked: 60 notes, the selection stays on screen.
  - Desktop seeks landed at the start of the movie (commit `d824319`). The player seeked to the previous keyframe; now it lands on the requested time. Checked in the desktop app, paused and playing.
- Deployed to the Surface server on 2026-10-20: the %locus desk, with the messages fixes and spam protection, after a ZFS snapshot (`rpool/urbit/pier@locus-desk-deploy-20261020T042207Z`). The desktop programs and the gateway were rebuilt from `139d8ca` and then `d824319`. The ship has been down since 2026-10-20, so the deployed agents are not running right now.
- Next:
  1. Decide on the factory reset of the comet (see Open above). Until then the server cannot run.
  2. Deploy the group conversations (built and tested, below) and the messages state-2 upgrade. The desk deploy and the desktop, web and CLI rebuild are ready; the desk deploy waits for the server.
  3. The author tries the whole set. This stage is done when they use it as their only system for notes, files and calendar/tasks.

## Later stages

Stages 3–7 (agents and feedback, the clan, clients, heavy compute, other clans) have not started. A command plane with members, named functions, approvals and an audit log already exists from earlier work. It is the starting point for stage 3.
