# Locus: status

Where the work stands against [plan.md](plan.md). The plan says what to do and [intent.md](intent.md) says why. This file only tracks progress, so update it when a stage moves. Last updated 2026-10-17.

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
  - Movies and Music: views over Files. A movie is a video file in `/Movies`; a track is an audio file in `/Music/<artist>/<album>/`. Play is tested in the browser and on the desktop, with video frames drawn in the app. Pause, seek, volume, next and previous are built but not yet tried.
  - Messages: direct messages between ships, sent as `%messages` pokes over Ames. A message you send shows as sending until the other ship's agent has it, then as delivered. Tested with messages to the same ship; not yet between two real ships.
- Performance: every core operation measured in release is under 16.6 ms (slowest: opening a folder of 10,000 files, about 10 ms). The shells draw long lists as virtual rows (Notes, Files, Contacts, Photos, Movies, Music, Messages). Todo and the calendar agenda still draw every row.
- Deployed to the Surface server on 2026-10-18: the browser app, the gateway, and the %locus desk with %messages and folder moves. The desk was snapshotted first (rpool/urbit/pier@locus-desk-deploy-20261018T190032Z). The installed desktop programs still have a GStreamer bug (no movie or track plays) until the fix in aece93a is built and switched in.
- Next:
  1. Deploy the latest build and check it on the server (needs the administrator's confirmation).
  2. Fix a race in moving a note between notebooks: typing during the move can be lost when the original is deleted.
  3. Virtual rows for Todo and the calendar agenda.
  4. Messages between two real ships, and group conversations (not started).
  5. The author tries the whole set. This stage is done when they use it as their only system for notes, files and calendar/tasks.

## Later stages

Stages 3–7 (agents and feedback, the clan, clients, heavy compute, other clans) have not started. A command plane with members, named functions, approvals and an audit log already exists from earlier work. It is the starting point for stage 3.
