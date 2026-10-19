# Locus: status

Where the work stands against [plan.md](plan.md). The plan says what to do and [intent.md](intent.md) says why. This file only tracks progress, so update it when a stage moves. Last updated 2026-10-18.

## Now: stages 1 and 2, a first version to try

The near-term goal is a version the author can use day to day: working browser and Linux desktop apps for notes, todo, calendar, files, photos and contacts, plus the `locus` command line for everything, running on the clan server. The agent builds and tests it end to end, then the author tries it.

### 1. Foundation (mostly done)

- Done: a NixOS server with ZFS, the Urbit ship as a service, and `locus-gateway` serving the browser app, file bytes and the ship's API. The first server runs it, and the Obsidian vaults are imported as notebooks.
- Open:
  - The server runs on a comet. The planet (the clan's root identity) comes later.
  - The ship's Azimuth eth-watcher error is unfixed.
  - Nobody has yet tried the "fresh machine through one documented procedure" check from the plan.

### 2. Core apps (in progress)

- Done and tested (unit tests, plus clicking through each app against real data with throwaway items):
  - Nine standard point-and-click apps. Each runs in its own browser tab (`/app/<app>`) and as its own desktop program (`locus-<app>`). Todo is a tab of the Calendar app, so it has no app of its own.
  - The `locus` CLI covers the same actions as the apps.
  - Notes: folders, notebooks, rename, move, delete, revision history and restore, move between notebooks with a destination picker, and conflict copies.
  - Todo: tasks are checkbox lines in notes. New tasks go to the notebook named "Tasks" if there is one, otherwise to the notebook being browsed.
  - Files: upload, folders, rename, move, delete. Folders can be renamed and moved (the `%files` desk has `dir.move`).
  - Photos: albums, viewer, rename, move, delete. A video in the library plays in the viewer, built but not yet tried.
  - Calendar and Contacts: create, edit, delete.
  - Movies and Music: views over Files. A movie is a video file at or below `/Movies`; a track is an audio file at or below `/Music` (in `/Music/<artist>/<album>/`, for example). Play is tested in the browser and on the desktop, with video frames drawn in the app. Pause, seek, volume, next and previous are built but not yet tried.
  - Messages: direct messages between ships, sent as `%messages` pokes over Ames. A message you send shows as sending until the other ship's agent has it, then as delivered. If the ship refuses it, it shows as failed and the text goes back into the draft. Messages you send to yourself are stored once. Conversations list newest first, and an open conversation clears its unread count. Tested with messages to the same ship (Hoon and Rust tests, including a remote poke that the previous build silently dropped).
  - Spam protection: a ship's incoming messages are refused when it is blocked (`locus messages block ~ship`, or the Block button in the conversation), and the 21st message from one ship within a minute is refused. A refused message stores nothing, and our own sends to a blocked ship still go. Built and tested; not deployed yet.
  - Not yet verified between two real ships. The fake-ship test could not get past its setup on 2026-10-18 (a fake ship's own Locus agents were down, and its API key could not be read). Still to run: a message both ways, a send to a ship that is offline (it should stay pending, not fail), and a junk poke (it should be refused and store nothing).
- Performance: every core operation measured in release is under 16.6 ms (slowest: opening a folder of 10,000 files, about 10 ms). The shells draw long lists as virtual rows (Notes, Files, Contacts, Photos, Movies, Music, Messages). Todo and the calendar agenda still draw every row.
- Deployed to the Surface server on 2026-10-18: the browser app, the gateway, and the %locus desk with %messages and folder moves. The desk was snapshotted first (rpool/urbit/pier@locus-desk-deploy-20261018T190032Z). The installed desktop programs still have a GStreamer bug (no movie or track plays) until the fix in aece93a is built and switched in.
- Committed but not deployed (on `main`, after 8c03e3c):
  - `a6ece4e` and `d28c44f`: remote messages arrive (they were dropped before); a bad poke or upgrade no longer wipes an agent's data (all six agents); ids above 999,999; newest-first conversations; a move from a folder that does not exist answers "not found"; the shell and browser fixes above.
  - `7d1a114`: the locus and notes Hoon test files compile (one shadowed binding renamed; a dead helper deleted).
  - `91e56a7` and `77e6bbd`: spam protection (above). The desk changes move `%messages` to state-1 with a migration from state-0, so a server holding the current state must load it on upgrade; the migration is tested on fake ships only.
- Next:
  1. Deploy the commits above and check every agent on the server. This needs the administrator's confirmation.
  2. Run the two-ship checks listed above, on real ships or fixed fake ones.
  3. Try the transport controls (pause, seek, volume, next, previous) and a video in the Photos viewer, in the desktop and browser apps.
  4. Fix a race in moving a note between notebooks: typing during the move can be lost when the original is deleted.
  5. Virtual rows for Todo and the calendar agenda.
  6. Group conversations (not started).
  7. The author tries the whole set. This stage is done when they use it as their only system for notes, files and calendar/tasks.

## Later stages

Stages 3–7 (agents and feedback, the clan, clients, heavy compute, other clans) have not started. A command plane with members, named functions, approvals and an audit log already exists from earlier work. It is the starting point for stage 3.
