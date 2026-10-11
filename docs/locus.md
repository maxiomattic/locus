# Locus command plane

Clan identity, named functions, OS-mutating commands behind human approval, append-only audit. Notes stay in `%notes`; Gall `%locus` lives on Clay desk `%locus` (same desk as `%notes` / `%calendar` / `%photos` / `%contacts`).

**Done when:** a second principal exists on the ship; `os.rebuild` from an agent queues and cannot approve; an owner/member can approve; `history.purge` is denied and the audit log still has entries; `locus` CLI talks to the same functions.

## Functions

| Name | Who |
|---|---|
| `household.list` / `household.add` / `household.grant` | list: any member; add/grant: owner |
| `household.issue-key` | owner — mint/rotate a member `X-Api-Key` (shown once) |
| `os.rebuild` / `os.rollback` | any member (including agent) — **queues**, does not run |
| `os.approve` / `os.deny` / `os.executed` | owner or member, **never agent** |
| `audit.list` | any member |
| `history.purge` / `zfs.destroy` | always denied |

`GET /locus/~/v1/whoami` returns the authenticated principal. Cookie or the owner `api-key` is the ship owner; a member key in the `keys` map is that ship. Agents must call with their own key. `--as` is owner-only (Hoon tests / running a worker from the owner session).

On init the ship (`our`) is the owner. Add moons/planets as `member` or `agent`. `household.add` returns `apiKey` once; `household.issue-key` rotates it.

The Gall agent does **not** run `nixos-rebuild`. Host-side `locus execute <id>` dry-activates unless `LOCUS_OS_EXECUTE=1`.

## Check on the bring-up VM

On the bring-up VM, rsync + OCC + this check is one command:

```bash
./scripts/server-vm.sh locus
```

Or after the desk is already committed: `EYRE_URL=http://127.0.0.1 ./scripts/locus-check.sh` on the guest (host Eyre is `http://127.0.0.1:8080`).

On the laptop, after a session exists:

```bash
nix develop
cargo run -p locus-command --bin locus -- help
# pipe +code or a 600 file:
locus login /path/to/code
locus add ~dev worker agent          # prints apiKey once
locus call os.rebuild '{"flake":".#server"}'   # owner session
# agent calls use X-Api-Key from add / issue-key, not --as
locus approve 1
# mutating switch is opt-in:
# LOCUS_OS_EXECUTE=1 locus execute 1
```

Default Eyre URL is `http://127.0.0.1:8080` (`LOCUS_EYRE_URL` or `NOTES_EYRE_URL`).
