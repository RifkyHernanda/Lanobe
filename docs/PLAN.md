# Lanobe — plan and progress

Living document. The plan was agreed before P0; the status block is updated as
phases land. Kept in the repo deliberately: it previously existed only in a
throwaway session workspace, which would have lost it on any machine change.

## Status

| Phase | State | Notes |
| --- | --- | --- |
| P0 — Fork and strip to LN-only | **done** | `45c3bb4` → `2222cc5` |
| P1 — app-server (auth + settings) | **done** | `563c115` |
| Docker + CI/CD (pulled forward from P6) | **done** | image at `ghcr.io/rifkyhernanda/lanobe` |
| P2 — Dictionary lookup performance | next | |
| P3 — Vocab, kanji bookmarks, auto-highlight | pending | the core feature |
| P4 — Anki `.apkg` export | pending | |
| P5 — Offline PWA and write queue | pending | |
| P6 — EC2 deploy (Caddy + TLS) | partial | Docker and CI done; TLS and provisioning remain |

### Known gaps

- No dictionaries ship with the app. Install them from the reader's settings
  modal, or copy an existing `yomitan.db` in. Import on a real machine, not on
  the t2.micro.
- `/api/app/meta` is fetched three times on startup; folded into P2.
- 524 `react-hooks` warnings in inherited reader code, surfaced once the plugin
  was actually registered. Some are likely real; worth a pass during P2/P3.
- 8 deinflector tests fail (they fail on upstream too) — marked `#[ignore]`.

---

# Lanobe — a light-novel-only fork of Manatan

## Context

You self-host Manatan and like it, but you only ever use the light-novel half. The
manga/anime machinery isn't just unused — it's actively hostile to the target
deployment: `bin/manatan/src/main.rs` spawns a **Suwayomi Java child process** and
pulls in `manatan-server-public` (which carries a CEF/Chromium runtime for source
extensions). On a t2.micro (1 vCPU, 1 GB RAM) the JVM alone would consume the box.

Three things you want that Manatan doesn't do well today:

1. **Dictionary popup latency.** Every tap is an HTTP round-trip to
   `/api/yomitan/lookup`, which does a SQLite query plus deinflection with no cache
   (`crates/yomitan-server/src/lookup.rs`, `state.rs`). On localhost that's tolerable;
   over the internet to EC2 it will not be.
2. **No kanji bookmarking.** There is a highlight system
   (`features/ln/reader/hooks/useHighlights.ts`) but it stores free-text selections
   per book, with no vocabulary concept, no cross-book index, and no review surface.
3. **No end-of-book Anki deck.** `Manatan/utils/anki.ts` speaks AnkiConnect only —
   useless from a tablet with no Anki running.

The outcome: a single small Rust binary + React PWA, LN-only, that runs comfortably on
your t2.micro behind HTTPS, is fast to look words up in from any of your three devices,
remembers the kanji you don't know and marks them everywhere, and hands you an `.apkg`
when you finish a book.

**Project name:** `lanobe`. Fork of `KolbyML/Manatan` (MIT) at the currently checked-out
rev `d4ecef3`.

---

## Decisions taken

| Question | Decision |
|---|---|
| Fork scope | Hard fork, LN-only. Delete manga/anime/OCR/extensions and the Java runtime. |
| Save granularity | Save the **term** and index **each kanji** in it. Highlights are **global** across the library. |
| Anki | Generate `.apkg` server-side (default); keep AnkiConnect as an alternate target. |
| Lookup speed | Server LRU + batch endpoint + client prefetch of the visible page + IndexedDB cache. |
| Offline | Full offline reading; writes queue and flush on reconnect. |
| Auth | Built-in session login (argon2 + signed cookie), Caddy for TLS. |
| Hosting | Multi-stage image built in GitHub Actions → GHCR → `docker compose pull` on EC2. |

---

## What survives the cut

**Rust crates kept:** `novel-server` (EPUB library, progress, categories, fonts — sled),
`yomitan-server` (dictionary + deinflector, SQLite), `audio-server` (word audio for cards).

**Rust crates/deps removed:** `manatan-server-public` (Suwayomi + CEF), `ocr-server`,
`eframe`/`tao` (desktop launcher GUI), `self_update`, the `resolve_java` /
`suwayomi_proc` spawn path in `main.rs`, `bin/manatan_android`.
`sync-server` stays compiled (`novel-server` depends on it) but its Google-Drive routes
come off the router — with a central server it's redundant.

**Frontend kept:** `features/ln` (14k LOC — the reader is already good: tategaki,
furigana, paged/scroll/virtual modes, click zones, per-language settings),
`features/dictionary`, `features/theme`, `features/metadata` (repointed),
`features/settings` (trimmed), `src/Manatan/*` (popup, DictionaryView, Anki, audio),
`src/base`, `src/lib`.

**Frontend deleted:** `features/{anime,manga,chapter,reader,browse,source,extension,
downloads,updates,migration,tracker,global-search,library,category,membership,device}`
— roughly 40k LOC — plus their routes in `App.tsx` and entries in
`features/navigation-bar`.

---

## Architecture

```
┌─ Caddy (TLS, :443) ──────────────────────────────┐
│  └─ lanobe (axum, :4567)                         │
│       /api/app      → app-server   (auth, meta)  │  NEW
│       /api/novel    → novel-server (sled)        │  kept
│       /api/yomitan  → yomitan-server (sqlite)    │  kept + cache/batch
│       /api/audio    → audio-server               │  kept
│       /api/study    → study-server (sqlite)      │  NEW
│       /*            → embedded React PWA         │
└──────────────────────────────────────────────────┘
```

Router assembly lives at `bin/manatan/src/main.rs:1184-1215` — that block is the
template for the new one; everything above it in `run_server` (Suwayomi config, java
bridge preflight, `build_state`) goes away.

### New crate: `crates/app-server`

Replaces the Suwayomi endpoints the surviving frontend still needs. `app.db` (SQLite).

- `POST /api/app/login`, `POST /api/app/logout`, `GET /api/app/session`
- `GET|PUT /api/app/meta` — global key/value, the replacement for Suwayomi's
  `meta/global` that `useMetadataServerSettings` reads. **This is the one non-obvious
  coupling**: `features/theme/AppThemeContext.tsx`, `CreateThemeDialog.tsx`,
  `ThemeList.tsx` and most of `features/settings` go through it.

Auth: password hash in `LANOBE_PASSWORD_HASH` (argon2id), HMAC-signed cookie keyed by
`LANOBE_SECRET`, 90-day expiry. An axum middleware layer guards `/api/*` except
`/api/app/login`. Frontend reuses `features/authentication`; `RequestManager` gets a
401 interceptor that routes to the login page.

### New crate: `crates/study-server`

`study.db` (SQLite — sled is wrong here, these need queries and joins).

```sql
saved_term(id, term, reading, gloss_json, book_id, chapter_index,
           sentence, created_at, status)        -- status: unknown|learning|known
saved_kanji(char PRIMARY KEY, first_term_id, created_at, status)
term_kanji(term_id, char)                       -- join table, drives kanji indexing
export_log(id, book_id, exported_at, term_ids_json)
```

Routes: CRUD on `/api/study/terms` and `/api/study/kanji`; `GET
/api/study/highlight-index` returning a compact `{kanji:[…], terms:[…]}` payload with an
**ETag** (this is the cached artifact the reader consumes); `GET /api/study/stats`;
`POST /api/study/export/apkg`.

---

## Work phases

### P0 — Fork and strip

Ruthlessly, before writing any new feature. Everything downstream is easier in a small
tree.

1. Rust: drop workspace members and deps; rewrite `bin/manatan/src/main.rs` (1632 LOC →
   ~250) keeping only: clap CLI (trimmed to host/port/data-dir/novel-path), tracing
   init, `resolve_data_dir`, router assembly, `serve_react_app` (the `RustEmbed`
   `FrontendAssets` fallback), CORS layer. Delete the `eframe::run_native` branch — the
   binary is always headless.
2. Frontend: delete the feature directories listed above, then let `tsc` drive the
   cleanup. `RequestManager.ts` is 4500 LOC and mostly Suwayomi REST — delete method
   groups incrementally against the type errors rather than rewriting it.
3. Repoint `features/metadata/services` at `/api/app/meta`.
4. Success criterion for P0: `cargo build --release` and `yarn build` both clean, app
   boots, you can import an EPUB and read it. Nothing new added yet.

Licensing note: Manatan is MIT (keep `LICENSE` + attribution), but many `WebUI/src`
files carry **MPL-2.0** headers from Suwayomi. MPL is per-file copyleft — keep the
headers and publish modified versions of those files. Don't strip the headers.

### P1 — Auth + app-server

Build `crates/app-server`, wire the middleware, port the theme/settings metadata reads.

### P2 — Lookup performance

Server (`crates/yomitan-server`):
- `POST /api/yomitan/lookup/batch` taking `{texts: [...], language}`.
- An LRU (moka, or a hand-rolled `RwLock<LruCache>`) keyed by
  `(text, index, group, language)`, sized ~50 MB.
- SQLite pragmas in `state.rs:60-70`: `mmap_size`, `cache_size = -65536` (64 MB),
  `temp_store = MEMORY`. Note `journal_mode = DELETE` is set deliberately (Android);
  make WAL conditional on desktop/server rather than flipping it blindly.

Client:
- Wrap `lookupYomitan` (`src/Manatan/utils/api.ts:81`) in a `LookupCache`: in-memory Map
  in front of a localforage/IndexedDB store, keyed the same way.
- New `features/ln/reader/hooks/useLookupPrefetch.ts`: when a chapter/page renders,
  walk the visible blocks, extract candidate terms, and batch-fetch the uncached ones at
  idle priority (`requestIdleCallback`). The existing `useTextLookup.ts` already has all
  the tokenizing/boundary logic to borrow from.

Target: tap→popup under 50 ms for anything on screen, since it's a cache hit.

### P3 — Vocab, kanji bookmarks, auto-highlight

The core feature. Three parts:

**Save.** Add a save control to `src/Manatan/components/DictionaryView.tsx` (1478 LOC —
the popup). On save, POST the term with its reading, glossary, containing sentence, book
and chapter; the server extracts kanji from the term and populates `saved_kanji` +
`term_kanji`.

**Review surface.** New `features/study/screens/SavedScreen.tsx` with **Kanji** and
**Words** tabs: search, filter by book, sort by date/frequency, bulk status change
(`unknown` → `known` removes it from highlighting), delete. New nav entry.

**Highlight engine.** `features/ln/reader/utils/injectHighlights.ts` currently does
`indexOf` on raw HTML strings — fragile (it can inject into attributes) and O(n·m).
Replace it:
- Fetch `/api/study/highlight-index` once per session, cache in IndexedDB by ETag.
- Build a matcher once per book: a `Set<char>` for kanji plus a trie for multi-char terms.
- Apply per block with a `TreeWalker` over text nodes (skipping `rt`/`rp`, the same
  filter `useTextLookup.ts:createVisibleTextWalker` already uses), wrapping hits in
  `<mark class="saved-kanji">` / `<mark class="saved-term">`.
- Memoize per `blockId + indexETag` so scrolling and re-renders are free.

Keep the existing manual-selection highlights as a separate, coexisting layer.

Settings: highlight kanji / highlight terms / only-unknown / style (background vs
underline vs color).

### P4 — Anki export

- `POST /api/study/export/apkg` in `study-server`, using `genanki-rs`. Note type fields:
  Expression, Reading, Glossary, Sentence, SourceBook, Audio. Word audio is pulled from
  the existing `audio-server` and embedded as deck media. Deck name `Lanobe::{book}`.
  `export_log` prevents re-exporting the same terms.
- Book-completion hook in `features/ln/reader/screens/LNReaderScreen.tsx`: at 100%
  progress, offer "Export N new words".
- AnkiConnect stays as an alternate target selectable in settings — the existing
  `Manatan/utils/anki.ts` needs no changes.

### P5 — Offline PWA

- `vite.config.ts` already has `VitePWA` with `globPatterns: []` (nothing precached).
  Precache the app shell; add runtime caching for `/api/novel/content/*`,
  `/api/novel/file/*`, `/api/study/highlight-index`.
- Outbox in `lib/storage/AppStorage.ts`: queue progress updates and term saves while
  offline, flush on the `online` event. `getDeviceId()` already exists there.
- Conflict rules: progress → highest `totalCharsRead` per book wins; saved terms → union
  with tombstones for deletes.

### P6 — Deployment

Replace the current `Dockerfile` (which just unpacks prebuilt release tarballs) with a
real multi-stage build:

1. `node:22` → `yarn build` the WebUI
2. `rust:1.88` + cargo-chef → `cargo build --release` with the built WebUI embedded
3. `debian:bookworm-slim` → binary + CA certs only

`linux/amd64` only (t2.micro is x86_64). GitHub Actions builds and pushes to
`ghcr.io/<you>/lanobe` on tag; EC2 runs `docker compose pull && up -d`.

`docker-compose.yml` on the box: `lanobe` + `caddy` (automatic Let's Encrypt on your
domain), volumes for `./data` (novel.db, yomitan.db, study.db, app.db, EPUBs) and Caddy
state.

**t2.micro reality check** — worth doing before you commit to the instance:
- 1 GB RAM, no swap by default → **add a 2 GB swapfile**.
- Expected steady-state: binary ~60-100 MB RSS, SQLite page cache capped at 64 MB,
  Caddy ~20 MB. Fits, with room.
- The default 8 GB gp3 volume is the real risk — a full Yomitan dictionary set
  (JMdict + frequency + pitch + kanji dicts) can run to several GB after import.
  **Provision 20-30 GB.**
- **Do not import dictionaries on the instance.** Import them locally, then `scp` the
  built `yomitan.db` up. Import is CPU- and RAM-heavy and will thrash a t2.micro's
  burst credits.

---

## Verification

- `cargo clippy --all-targets -- -D warnings` and `cargo build --release` clean;
  `yarn lint` and `yarn build` clean.
- `cargo test -p manatan-yomitan-server` — the deinflector has an existing test suite
  (`crates/yomitan-server/src/deinflector/tests.rs`) that must stay green through the
  cache work. `features/ln/services/discoveredEpubImport.test.ts` and
  `features/ln/utils/progressStatus.test.ts` likewise.
- New tests: study-server CRUD + kanji extraction; highlight-matcher unit tests
  (overlapping terms, terms inside ruby, terms spanning inline tags); apkg round-trip
  (generate, unzip, assert collection schema).
- **Memory proof:** run the image locally with `--memory=1g --cpus=1` and read a full
  chapter while watching `docker stats`. If it OOMs there, it OOMs on EC2.
- **Latency proof:** instrument tap→popup-painted; assert p95 < 50 ms warm, and compare
  cold vs. prefetched on a throttled (Fast 3G) profile.
- End-to-end smoke on the real deployment: import EPUB → read → tap unknown word → save
  → open a *different* book and confirm the kanji is highlighted → finish book → export
  `.apkg` → import into Anki and confirm fields and audio.
- Multi-device: read to 40% on laptop, open on phone, confirm position; go airplane
  mode, read and save two words, reconnect, confirm both landed.

---

## Open items (not blocking the start)

- Whether to keep `features/history` and repurpose it as LN reading history/stats, or
  delete it in P0 and add reading stats later as part of `study-server`.
- SRS scheduling inside Lanobe (the `review_state` table is stubbed for it) — out of
  scope for now; Anki does this.
- LNReader/Mangayomi source extensions for downloading novels — deliberately out of
  scope; the library is local EPUBs only.
