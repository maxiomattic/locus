# Mail and calendar (P2)

Canonical mail and calendar live in Gall on **our** `%locus` desk. A host SMTP/IMAP (or CalDAV) process is a **bridge** into those agents, not a second mailbox.

Survey (same job we did for notes):

- **rolrup/mail** — real Gall mailbox, but inbound is Cloudflare Email Routing + a Python bridge + Mailgun. That is a second mailer. Do not fork it.
- **taalhavras/ucal** — Gall calendar store. We did not install it as its own desk; types are small enough to keep on `%locus` next to `%notes` / Gall `%locus`.

**Done when:** create/list/`expectedRevision` conflict pass for a message and an event against **our** desk; ingest of a fixture `.eml` lands in inbox; `+vats` still does not show Tlon/`%groups`.

## Layout

| Path | Role |
|---|---|
| `desk/app/mail.hoon` | Gall mail store (`/mail`) |
| `desk/app/calendar.hoon` | Gall calendar store (`/calendar`) |
| `desk/sur/mail.hoon` / `desk/sur/calendar.hoon` | Types |
| `crates/mail-protocol` | HTTP client |
| `crates/mail-bridge` | RFC822 ingest + dry-run SMTP (`mail-bridge ingest file.eml`) |
| `scripts/mail-p2.sh` | Live check |
| `scripts/fixtures/p2-ingest.eml` | Inbound fixture |

Auth is the Eyre cookie or `X-Api-Key` minted on init (scry `/~/scry/mail/v0/api-key.json`, no extra `x`).

## Check on the bring-up VM

```bash
./scripts/server-vm.sh p2
```

Or after the desk is committed: `EYRE_URL=http://127.0.0.1 ./scripts/mail-p2.sh` on the guest.

The bridge does not keep SQLite. Client cache (M6 shape) and a libcosmic window (M7) are later P2 slices, parallel with P3 COSMIC.
