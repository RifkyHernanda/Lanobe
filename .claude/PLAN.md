# Lanobe — plan and checkpoints

Living document. Tick checkpoints as they land. See `SPEC.md` for what each phase
is meant to produce and why.

## Status

| Phase | State |
| --- | --- |
| P0 — Fork and strip to LN-only | **done** |
| P1 — app-server: auth + settings | **done** |
| Docker + CI/CD (pulled forward from P6) | **done** |
| P2 — Dictionary lookup performance | **next** |
| P3 — Vocab, kanji bookmarks, auto-highlight | pending — *the core feature* |
| P4 — Anki `.apkg` export | pending |
| P5 — Offline PWA and write queue | pending |
| P6 — EC2 deploy: Caddy + TLS | partial |

---

## P0 — Fork and strip ✅

- [x] Fork Manatan at `d4ecef3`, keep the pristine import as commit 1
- [x] Drop `manatan-server-public` (Suwayomi + CEF) and the Java child process
- [x] Drop `ocr-server`, the Android bin, the eframe desktop launcher
- [x] Rewrite `main.rs` (1632 → ~215 lines), always headless
- [x] Delete ~40k lines of manga/anime frontend
- [x] Replace `RequestManager` (5077 → ~170 lines)
- [x] Replace Suwayomi `meta/global` settings store
- [x] `cargo build` and `vite build` clean; 21 MB binary, 17 MB idle RSS

## P1 — app-server ✅

- [x] `crates/app-server`, SQLite `app.db`
- [x] argon2id password, HMAC-signed session cookie, constant-time compare
- [x] Secret persisted so restarts don't log devices out
- [x] Auth opt-in: no password → open, with a startup warning
- [x] `/api/app/session|login|logout|meta`
- [x] `lanobe hash-password` reading stdin
- [x] 12 tests: forgery, tampered expiry, expiry, cookie parsing, settings merge
- [x] Verified with curl in both modes

## Docker + CI ✅

- [x] Multi-stage Dockerfile, bundled sqlite, ~21 MB binary on debian-slim
- [x] `docker-compose.yml` (build) and `docker-compose.ghcr.yml` (pull)
- [x] CI: fmt, clippy `-D warnings`, `cargo test --workspace`, lint, vite build
- [x] Smoke test: every nav route in headless Chromium
- [x] Publish to GHCR, gated on all checks
- [x] Rust toolchain pinned (a floating `stable` broke the build once)

## P2 — Lookup performance ⬅ next

The complaint that started the project: the popup is slow, and it gets worse over
the internet to EC2.

- [ ] **Baseline first** — measure tap→popup with a dictionary actually installed.
      Do not optimise an unmeasured path.
- [ ] Server: LRU cache keyed `(text, index, group, language)`, ~50 MB
- [ ] Server: `POST /api/yomitan/lookup/batch`
- [ ] Server: SQLite `mmap_size`, `cache_size = -65536`, `temp_store = MEMORY`
      (note: `journal_mode = DELETE` is deliberate for Android — make WAL conditional)
- [ ] Client: `LookupCache` — in-memory map over IndexedDB, same key
- [ ] Client: `useLookupPrefetch` — batch-fetch visible blocks at idle priority
- [ ] Fix `/api/app/meta` being fetched 3× on startup
- [ ] **Target: p95 < 50 ms warm.** Verify on a throttled profile, not localhost.
- [ ] Revisit the 8 ignored deinflector tests — they may be hurting lookup quality

## P3 — Vocab, kanji bookmarks, highlighting

The core feature. Nothing else in the project matters as much.

- [ ] `crates/study-server`, `study.db`, schema per SPEC §4
- [ ] `POST /api/study/terms` — store term, index each kanji
- [ ] `GET /api/study/highlight-index` with ETag
- [ ] CRUD + `GET /api/study/stats`
- [ ] Save control in the lookup popup (`Manatan/components/DictionaryView.tsx`)
- [ ] `Saved` screen: Kanji | Words tabs, search, filter, mark known, delete
- [ ] **Replace `injectHighlights.ts`** — it does `indexOf` on raw HTML and can
      inject into attributes. TreeWalker over text nodes, skipping `rt`/`rp`.
- [ ] Matcher: `Set` for kanji + trie for terms, built once per book
- [ ] Memoise per `blockId + indexETag`
- [ ] Settings: highlight kanji / terms / only-unknown, and style
- [ ] Tests: overlapping terms, terms inside ruby, terms spanning inline tags

## P4 — Anki export

- [ ] `POST /api/study/export/apkg` via `genanki-rs`
- [ ] Fields: Expression, Reading, Glossary, Sentence, SourceBook, Audio
- [ ] Embed word audio from `audio-server` as deck media
- [ ] Deck `Lanobe::{book}`; `export_log` prevents duplicates
- [ ] Book-completion prompt at 100%
- [ ] Keep AnkiConnect as an alternate target
- [ ] Test: generate, unzip, assert collection schema

## P5 — Offline

- [ ] VitePWA precache; runtime caching for book content + highlight index
- [ ] Outbox in `AppStorage`, flush on `online`
- [ ] Conflicts: progress → max `totalCharsRead`; terms → union + tombstones
- [ ] Verify: read offline, save two words, reconnect, both land

## P6 — EC2

- [x] Image published to GHCR
- [ ] `docker-compose.prod.yml` with Caddy + automatic TLS
- [ ] Provisioning notes: 2 GB swap, 20–30 GB volume
- [ ] Copy `yomitan.db` up rather than importing on the instance
- [ ] Set `LANOBE_PASSWORD_HASH` **before** exposing it
- [ ] Verify under `docker run --memory=1g --cpus=1` — if it OOMs there, it OOMs on t2.micro

---

## Known issues

- **No dictionary ships with the app.** First run offers to install one; otherwise
  copy an existing `yomitan.db` in.
- The setup wizard's third step configures anime subtitles — meaningless here,
  should be dropped.
- 524 `react-hooks` warnings in inherited reader code, surfaced once the plugin was
  actually registered in eslint. Some are likely real.
- 8 deinflector tests fail; they fail on upstream too. Marked `#[ignore]`.
- `SettingsInjector` is dead code: it portals into the removed manga reader's
  toolbar. Either delete it or repoint it at the LN reader.

## Bugs found and fixed (worth not repeating)

| Bug | Cause | Lesson |
| --- | --- | --- |
| Blank page | Circular import → TDZ on the `RestClient` class | Type imports that no longer exist become *runtime* edges |
| Stuck on splash | Session probe queued behind auth it was meant to initialise | Bypass lists must name real endpoints |
| `/settings` crash | Links to routes deleted in P0b | Delete the links with the routes |
| `/more` crash | Four identifiers used, never imported — broken upstream too | Inherit bugs knowingly |
| No dictionary UI | `OCRManager.tsx` deleted; it also hosted the setup wizard | Check what else a file renders before deleting it |
| CI red on arrival | Floating `stable` toolchain + `-D warnings`; `yarn lint` never run | Never add a gate you have not run |
