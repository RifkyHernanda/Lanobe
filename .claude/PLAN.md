# Lanobe — plan and checkpoints

Living document. Tick checkpoints as they land. See `SPEC.md` for what each phase
is meant to produce and why.

## Status

| Phase | State |
| --- | --- |
| P0 — Fork and strip to LN-only | **done** |
| P1 — app-server: auth + settings | **done** |
| Docker + CI/CD (pulled forward from P6) | **done** |
| Rebrand to Lanobe | **done** |
| P2 — Dictionary lookup performance | **re-scoped after measuring; transport done, client cache deferred** |
| P3 — Vocab, kanji bookmarks, auto-highlight | **next** — *the core feature* |
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

## P2 — Lookup performance (re-scoped)

The complaint that started the project: the popup is slow, and it gets worse over
the internet to EC2.

- [x] **Baseline first** — measured with JMdict installed (515,737 terms, 177 MB)
      and a real EPUB. The result contradicted the rest of this phase.

| Measurement | Result |
| --- | --- |
| Server lookup, 50 distinct words | p50 0.9 ms, **p95 1.2 ms** |
| Same words, second pass | p95 1.2 ms — *identical*, so nothing to warm |
| Query plan | `SEARCH terms USING INDEX idx_term_dict (term=?)` |
| Warm RTT to the EC2 box | **60–107 ms** |
| Lookup payload | 5.3–33.8 KB, **uncompressed** |
| Main JS bundle | 897 KB, **uncompressed** |
| `Cache-Control`/`ETag` on anything | **none** |

The server does ~1.2 ms of work inside a 60–107 ms round trip — **1–2% of what
you feel.** The planned server-side LRU would have optimised that 1.2 ms while
spending ~50 MB of RAM on a 1 GB box. What actually cost: no compression and no
cacheability. The reference Manatan deploy hides this because Cloudflare
compresses at its edge; a bare Caddy in front of Lanobe would not.

- [x] Serve `Cache-Control` + weak `ETag`, with `If-None-Match` → 304.
      897 KB → 0 bytes on revalidation. Hashed `assets/*` are `immutable`;
      `index.html`, `sw.js` and `locales/*` must stay `no-cache`.
- [x] gzip responses. Bundle 896,458 → 283,906 B; lookups 5.6–10.7× smaller.
      EPUBs excluded — they are already-deflated zips typed as octet-stream.
- [ ] Client: `LookupCache` — designed, deferred until after P3. **Key on the
      verbatim 24-code-point window at the cursor**, which is exactly what
      `lookup.rs:121` scans. Do *not* key on the matched headword and probe
      prefixes: a cached bare `大` would hijack a later tap on `大学` and
      underline one character instead of two. Wrong, not merely stale.
- [ ] Client: chain-prefetch from `match_len` after each tap; whole-page prefetch
      needs `POST /api/yomitan/lookup/batch` and is payload-bound, not
      request-bound — measure the payload before building it.
- [ ] Fix `/api/app/meta` being fetched 3× on startup
- [ ] **Target: p95 < 50 ms warm.** Verify on a throttled profile, not localhost.
- [ ] Revisit the 8 ignored deinflector tests — they may be hurting lookup quality

**Moved to P6, not dropped:** the server-side LRU and the SQLite pragmas. The
1.2 ms was measured on an 18 GB laptop against a DB that fits in page cache. On a
t2.micro with multi-GB dictionaries the same query hits EBS. Re-measure there
before reviving either.

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
- [ ] **Re-measure lookup latency on the instance**, then decide whether the
      server-side LRU cache and SQLite `cache_size`/`mmap_size` tuning are worth
      their RAM. Deferred here from P2 because they were unjustifiable against a
      1.2 ms baseline on a laptop, but a multi-GB dictionary set on 1 GB of RAM is
      a genuinely different question.
- [ ] `docker-compose.prod.yml` with Caddy + automatic TLS
- [ ] Provisioning notes: 2 GB swap, 20–30 GB volume
- [ ] Copy `yomitan.db` up rather than importing on the instance
- [ ] Set `LANOBE_PASSWORD_HASH` **before** exposing it
- [ ] Verify under `docker run --memory=1g --cpus=1` — if it OOMs there, it OOMs on t2.micro

---

## Known issues

- **No dictionary ships with the app.** First run offers to install one; otherwise
  copy an existing `yomitan.db` in.
- **`/saved` has no route.** `AppRoutes.saved` exists and the nav bar links to it,
  but `App.tsx` never registers a `<Route>`, so it falls through `matchAll` and
  silently redirects to the library. The smoke test cannot see this: it only
  asserts the page rendered *something*, and the library renders. P3 fixes it.
- **The dictionary-import "loading" state is dead code.** `apiRequest` throws on
  any non-2xx (`Manatan/utils/api.ts:64`), but the import-in-progress response is
  `503 {"error":"loading"}`, so the `'loading'` check below it never runs and all
  four call sites' `systemLoading` branches are unreachable. During an import you
  get empty results rather than "still importing".
- **A slow lookup can overwrite a newer popup.** `useTextLookup.ts` has no
  sequence guard and no `AbortController`. Tap A then tap B; if A resolves second
  it merges into B's state, giving B's popup position with A's contents.
- **Sentence furigana costs ~20 serialized round trips.**
  `japaneseFurigana.ts:303-350` awaits one lookup per token. At 60–107 ms RTT
  that is 1.2–2.0 s per sentence — larger than anything else measured. Needs
  cancellation, not just caching.
- **`make check` omits `--workspace`.** It runs `cargo clippy --all-targets` and
  `cargo test`, which is the exact trap documented in `CLAUDE.md` — `cargo test`
  alone runs zero tests, because `default-members` is `bin/lanobe`.
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
| Compose port forwarded nowhere | Host port changed to `5678:5678`; the container only ever listens on 4567 | Change only the left side of a port mapping |
| Container crash-looped on a fresh clone | Docker created the `./data` bind-mount source as root; the image's `chown` is masked by the mount | `docker compose up` reporting "Started" says nothing — check `ps` for `Restarting` |
| P2 aimed at the wrong 1.2 ms | Assumed the server was slow without measuring it | The phase said "baseline first" for a reason; the answer was compression and cache headers, neither of which was in the plan |
