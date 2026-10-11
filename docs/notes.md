# Notes desk

Hard fork of Tlon `%notes` (MIT, tloncorp/tlon-apps) onto **our** desk. This repo does not install Tlon Messenger or `%groups`.

Live check: [scripts/notes-occ.sh](../scripts/notes-occ.sh) — create personal notebook, folder, note; list; save with matching `expectedRevision`; stale save must return `%conflict`. `+vats` / the desk set must not include `%groups`.

## Layout

| Path | Role |
|---|---|
| `desk/app/notes.hoon` | Gall agent (groups / activity / mcp-proxy / Tlon UI stripped) |
| `desk/sur/notes.hoon` | Types, including `expectedRevision` / `%conflict` |
| `desk/lib/notes/json.hoon` | JSON |
| `desk/mar/notes/` | Marks |
| `desk/app/notes/openapi.json` | v1 HTTP spec |
| `desk/desk.bill` | starts **`%notes`**, **`%locus`**, **`%calendar`**, **`%photos`**, **`%contacts`** |
| `desk/NOTICE` | Tlon MIT pin + urbit 408k helpers |

Desk name on the ship: `%locus` (Gall agents `%notes`, `%locus`, `%calendar`, `%photos`, `%contacts`). Kelvin `[%zuse 408]`.

## Apply on the server

Eyre must already be on the LAN (`docs/urbit.md`). Pier stays on `rpool/urbit/pier`. Do not boot a fake ship on a non-ZFS disk.

```bash
# on the server, from this repo
EYRE_URL=http://127.0.0.1 ./scripts/notes-occ.sh
```

The script `|merge`s `%locus` from `%base`, mounts, copies `desk/`, `|commit`s, `|install`s, then logs in with `sudo -u urbit urbit-code` (not `/~/login` HTML) and runs the OCC curls.

Khan/`urbit eval` prints loobeans as `0` (`%.y`) and `1` (`%.n`). The OCC waiter looks for `%notes-hoon-yes`, not the literal `%.y`.

From another LAN host, only after that commit:

```bash
EYRE_URL=http://<server-lan-ip> SKIP_COMMIT=1 ./scripts/notes-occ.sh
```

`SKIP_COMMIT=1` still needs +code via `urbit-code` or `NOTES_CODE_FILE` (a 600 file, not in git).

## Client on this laptop

The COSMIC window on the NixOS host talks to the ship in the bring-up VM:

```bash
nix develop
# VM already up: ./scripts/server-vm.sh up
cargo run --release -p notes-app
```

Default server URL is `http://127.0.0.1:8080`. **Get code from VM** SSHs into the guest for the login code. After the first successful sign-in, the window stores the API key under `~/.local/share/org.digital-workspace.notes/` and reconnects without pasting the code again.

There is no Save. Typing a word (or pausing ~1s) writes a copy-on-write version into `cache.sqlite`. The ship keeps the latest head.

The editor styles GitHub Flavored Markdown in the source: `# hello` shows a small dim hash and large heading text; `**bold**` keeps the stars. Spec is [GFM](https://github.github.com/gfm/) plus `[[wikilinks]]` / `![[embed]]`. Optional Preview renders the same spec (including images and tables). Right-click a note to delete. Ctrl+Z / Ctrl+Shift+Z undo and redo typing; **Note → Version history** restores a local snapshot. A conflict dialog keeps both copies by default. Same functions: `cargo run -p notes-sync --bin notes -- help`. No tags.

## Vault testing (not the product)

The notes window’s source of truth is the ship plus `cache.sqlite`, not a folder, so a symlink of the Obsidian vault would not fill the sidebar. For real-world data, two-way sync maps `~/Documents/new` (or `NOTES_VAULT` / `OBSIDIAN_VAULT_PATH`) onto local note ids ≥ 1_000_000_000.

- Poll ~1s in the window; or `notes vault-sync`.
- App edits write back to the `.md` file; Obsidian edits appear in the list. Both dirty → keep-both + CoW snapshot.
- Vault notes are **not** pushed to the ship. In-app delete does not remove vault files.
- New notes created in the app stay ship-backed.
