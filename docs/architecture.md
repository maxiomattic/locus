# Locus: architecture

High-level decisions that serve [intent.md](intent.md). These are expected to change. Details belong in the code.

## Decisions

1. **Clan identity.** Each clan has one Urbit planet as its root identity, with one administrator. Additional servers and devices get moons. Members and agents start as accounts on the clan's ship and can become their own ships later.
2. **Server stack.** NixOS for a declarative system with rollback, ZFS for snapshots, and an Urbit ship holding the clan's state. Software that can't live in Urbit (AI inference, games, 3D, video) runs on the host and is launched through Urbit.
3. **Data.** Urbit holds structured data and catalogs. Large files are stored on ZFS and referenced from Urbit. Notes, files, tasks and events are linked objects. Tasks are lines of text inside notes, and events are structured and iCalendar-compatible.
4. **One action layer.** Every action is a named function. Keyboard, mouse and agents all call the same functions. Permissions are per member and per agent, and everything is audited.
5. **Safety.** Deletions and overwrites can always be recovered (Urbit history, ZFS snapshots). Changes to the operating system need a human's approval and can be rolled back.
6. **Sync.** The server is the source of truth. Clients keep an aggressive cache and a queue of offline writes. A stale write is detected and both versions are kept. Nothing is ever silently overwritten.
7. **Interfaces.** Built in Rust and keyboard-first. One framework-agnostic core (state, keymap, update logic) is shared by two thin shells: libcosmic on the desktop, and upstream iced compiled to wasm in the browser. libcosmic doesn't run in the browser yet (its iced fork's web backend is stale); fixing that as a hard fork would let one shell serve both. Android comes later.
8. **Components.** Small parts that each do one thing. Outside code is taken as hard forks of small pieces, never as soft forks of large projects.
9. **Distribution.** Locus ships as a Nix flake plus an Urbit desk. A clan follows upstream or points at its own fork. Every update can be rolled back.
10. **Feedback.** A request carries its context (app, version, recent actions), becomes an issue, an agent drafts a change and a preview, and the administrator approves it before it ships.
11. **Reachability.** Clans and devices reach each other over Urbit's peer-to-peer network. A tunnel is an acceptable pipe, but never a place where data is stored or identity is managed.
12. **Gateway.** A small host process next to the ship serves the browser app, stores file and photo bytes on ZFS addressed by SHA-256, and proxies the ship's API, so the browser sees one origin. It checks every byte request against the ship's login.

## Open questions

- When should members move from accounts to moons?
- Which runtime should serve local AI inference?
