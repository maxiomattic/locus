# Locus: status

Where the work stands against [plan.md](plan.md). The plan says what to do and [intent.md](intent.md) says why. This file only tracks progress, so update it when a stage moves. Last updated 2026-10-16.

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
  - Six standard point-and-click apps. Each runs in its own browser tab (`/app/<app>`) and as its own desktop program (`locus-<app>`).
  - The `locus` CLI covers the same actions as the apps.
  - Notes: folders, notebooks, rename, move, delete, revision history and restore, move between notebooks with a destination picker, and conflict copies.
  - Todo: tasks are checkbox lines in an Inbox note.
  - Files: upload, folders, rename, move, delete.
  - Photos: albums, viewer, rename, move, delete.
  - Calendar and Contacts: create, edit, delete.
- Built but not on the server yet: the server still runs an older build. Everything after it needs a deploy, which needs the administrator's sudo.
- Next:
  1. Deploy the latest build and check it on the server.
  2. Fix a race in moving a note between notebooks: typing during the move can be lost when the original is deleted.
  3. Folder rename and move in Files. The server has no endpoint for these yet.
  4. Decide which notebook new tasks go into. Today it is the first notebook.
  5. The author tries the whole set. This stage is done when they use it as their only system for notes, files and calendar/tasks.

## Later stages

Stages 3–7 (agents and feedback, the clan, clients, heavy compute, other clans) have not started. A command plane with members, named functions, approvals and an audit log already exists from earlier work. It is the starting point for stage 3.
