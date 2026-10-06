# Touch clients (P5)

Stock GrapheneOS talks to the **same Eyre APIs** as the COSMIC apps (notes, then mail/calendar, photos/contacts, command plane). It is not a nested COSMIC session. **P7** (a Graphene/AOSP fork) waits until these surfaces exist. Parent plan: [plan.md](plan.md).

**Done when (this slice):** a Nix module asserts COSMIC is off; `locus-touch` lists the same OpenAPI paths the desktop uses; photo paths follow the desktop catalog rules.

This is not an APK and not a Graphene build. The phone still uses stock Graphene + whatever HTTP client you install against Eyre (LAN or the M4 tunnel).

## Layout

| Path | Role |
|---|---|
| `crates/locus-touch` | Shared OpenAPI list; `is_nested_cosmic_session() == false` |
| `nix/modules/mobile.nix` | `digitalWorkspace.mobile` — writes `/etc/locus/touch-apis.json` |
| `nixosConfigurations.mobile` | Eval-only placeholder (not a phone image) |

## Check

```bash
./scripts/plan-check.sh
```

That evaluates `nixosConfigurations.mobile` (COSMIC off, touch API list present). It does not flash a device.

## Later P5 slices (not this pass)

- Touch-native notes, then mail/calendar, against the live ship
- Cookie / API key storage on the phone
- P7 fork once those apps exist
