# Photo library and contacts

Catalog (albums, tags, relative paths, OCC) lives in Gall on **our** `%locus` desk. JPEG/PNG **bytes** stay on ZFS at `/var/lib/locus/photos`. The client thumbnail cache is not a second library. Parent plan: [plan.md](plan.md).

**Done when (this slice):** create/list/`expectedRevision` conflict pass for a photo and a contact against **our** desk; `..` and absolute catalog paths are rejected; Disko has `rpool/locus/photos`; first paint of a grid can hit SQLite thumbs without talking to Eyre.

## Layout

| Path | Role |
|---|---|
| `desk/app/photos.hoon` | Gall catalog (`/photos`) |
| `desk/app/contacts.hoon` | Gall contacts (`/contacts`) |
| `nix/disko.nix` | `rpool/locus/photos` → `/var/lib/locus/photos` |
| `crates/photos-protocol` | HTTP client + path rules |
| `crates/photos-sync` | SQLite thumbnail cache |
| `scripts/photos-p4.sh` | Live OCC |

Auth is the Eyre cookie or `X-Api-Key` (scry `/~/scry/photos/v0/api-key.json`).

A catalog `path` is ZFS-relative (`holidays/pier.jpg`). The agent refuses `/...` and `..`. Copy bytes onto the dataset separately; the ship never stores the file.

## Check

Automated (no ship): `./scripts/check.sh` (cargo + Disko photos dataset).

On the bring-up VM, after the desk is committed:

```bash
./scripts/server-vm.sh p4
```

Or on the guest: `EYRE_URL=http://127.0.0.1 ./scripts/photos-p4.sh`

A libcosmic photo grid is a later slice. Instant grid means the thumb cache; this pass proves the cache and the catalog, not a window.
