# Lanobe — specification

A self-hosted Japanese light-novel reader with instant dictionary lookup, saved
kanji tracking, and Anki export. One server, read from laptop, phone and tablet.

Fork of [Manatan](https://github.com/KolbyML/Manatan) (MIT), which is itself built
on [Suwayomi](https://github.com/Suwayomi/Suwayomi-Server) (MPL-2.0).

---

## 1. Why this exists

Manatan does light novels well, but ships them alongside manga, anime, OCR and an
extension runtime. For a reader who only wants light novels that machinery is
pure cost — most sharply on the deployment target, a **t2.micro with 1 GB of RAM**,
where Manatan's embedded Suwayomi JVM alone would consume the box.

Three gaps in Manatan motivated the fork:

1. **Lookup latency.** Every tap is an uncached HTTP round-trip to a SQLite query
   plus deinflection. Tolerable on localhost, not over the internet to EC2.
2. **No kanji bookmarking.** There is a highlight system, but it stores free-text
   selections per book — no vocabulary concept, no cross-book index, no review.
3. **No Anki deck at the end of a book.** Only AnkiConnect, which is useless from
   a tablet with no Anki running.

## 2. Goals and non-goals

**Goals**

- Read EPUB light novels comfortably: vertical text, furigana, paged and scroll.
- Tap any word → instant reading, definition, pitch accent.
- Save words you don't know; every kanji in them is indexed.
- Saved kanji are highlighted **everywhere**, in every book, automatically.
- Finish a book → export the new vocabulary as an Anki `.apkg`.
- Read on laptop, phone and tablet against one server, with progress in sync.
- Run on a t2.micro behind HTTPS.

**Non-goals**

- Manga, anime, OCR, source extensions. Removed, not deferred.
- Machine translation. This is a *dictionary*: word → reading and meaning. It will
  not translate a sentence into English.
- Downloading novels from the internet. The library is local EPUBs.
- Multi-user. Single user, single password.
- An SRS inside Lanobe. Anki already does that well.

## 3. Architecture

```
Caddy (TLS :443)
  └── lanobe (single axum binary, :4567)
        /api/app      app-server      auth + settings          SQLite  app.db
        /api/novel    novel-server    EPUB library, progress   sled    novel.db
        /api/yomitan  yomitan-server  dictionary lookup        SQLite  yomitan.db
        /api/audio    audio-server    word audio
        /api/study    study-server    saved vocab (planned)    SQLite  study.db
        /*            embedded React PWA (rust-embed)
```

One binary, ~21 MB, ~17 MB idle RSS. The WebUI is compiled in, so there is no
static file server to configure and nothing to serve separately.

### Why these choices

| Decision | Reason |
| --- | --- |
| Hard fork, not a feature flag | ~40k lines of manga/anime frontend and a JVM would otherwise be carried forever. |
| Embedded WebUI | One artifact to deploy; no nginx, no path config. |
| SQLite, bundled | Queries and joins for vocabulary; `bundled` so no system sqlite is needed anywhere. |
| sled kept for novel-server | Inherited and working. Not worth a migration. |
| Auth opt-in | `docker compose up` on a laptop should not demand a password; the internet should. |
| `linux/amd64` only | The target is a t2.micro. arm64 doubles CI for nothing. |

## 4. Data model

### Settings — `app.db`

```sql
meta(key TEXT PRIMARY KEY, value TEXT NOT NULL, updated_at INTEGER)
secrets(key TEXT PRIMARY KEY, value BLOB NOT NULL)   -- session signing secret
```

Values are JSON, merged on write, so a client can change one setting without
sending the whole object back. The server is the source of truth; localStorage is
a first-paint cache and offline fallback, never a peer.

### Vocabulary — `study.db` (planned, P3)

```sql
saved_term(id, term, reading, gloss_json, book_id, chapter_index,
           sentence, created_at, status)     -- status: unknown | learning | known
saved_kanji(char PRIMARY KEY, first_term_id, created_at, status)
term_kanji(term_id, char)                    -- join table; drives kanji indexing
export_log(id, book_id, exported_at, term_ids_json)
```

Saving a term writes one `saved_term` row and one `saved_kanji` row per kanji in
it. `status = known` removes something from highlighting without deleting it.

## 5. Feature specification

### 5.1 Library

- EPUBs are discovered from the library directory on startup and importable
  through the UI.
- Grid of covers with reading status and progress.
- Categories, sort and filter (inherited, working).
- The metadata directory is still named `.manatan-metadata`, deliberately: point
  Lanobe at an existing Manatan library and the books and progress come across
  with no migration.

### 5.2 Reader

Inherited from Manatan and already good. Vertical (tategaki) and horizontal;
paged, scroll and virtualised modes; furigana toggle; font family, size, weight,
line height, letter spacing; per-edge margins; themes; click zones; swipe;
character-count progress; auto-bookmark.

### 5.3 Dictionary lookup

- Tap or hover a word → popup with reading, definitions, pitch accent, audio.
- Deinflection handles conjugated forms.
- Dictionaries are Yomitan-format, installed through the first-run wizard or
  imported as zips.
- **Nothing ships with a dictionary.** An empty install returns empty lookups.

**Performance (P2).** Currently one uncached round-trip per tap.

- Server: LRU cache keyed by `(text, index, group, language)`; a batch endpoint;
  SQLite `mmap_size`, `cache_size`, `temp_store` tuning.
- Client: in-memory map over IndexedDB, keyed identically; prefetch every term on
  the visible page at idle priority.
- Target: **p95 under 50 ms** for anything on screen, because it is a cache hit.

### 5.4 Saved kanji and vocabulary (P3 — the core feature)

**Saving.** The lookup popup gets a save control. Saving stores the term with its
reading, glossary, containing sentence, book and chapter, and indexes every kanji
in it.

**Review.** A `Saved` screen with **Kanji** and **Words** tabs: search, filter by
book, sort by date or frequency, bulk status change, delete.

**Highlighting.** Saved kanji and terms are marked in *every* book, not just the
one they were saved from.

- `GET /api/study/highlight-index` returns a compact `{kanji, terms}` payload with
  an ETag; the client caches it in IndexedDB.
- A matcher is built once per book: a `Set` of kanji plus a trie for multi-char terms.
- Applied per block with a `TreeWalker` over text nodes, skipping `rt`/`rp` so
  furigana is never matched, wrapping hits in `<mark>`.
- Memoised per `blockId + indexETag`, so scrolling and re-renders are free.

This replaces the inherited `injectHighlights.ts`, which does `indexOf` on raw
HTML — fragile enough to inject into attributes, and O(n·m).

Settings: highlight kanji / terms / only-unknown, and style.

### 5.5 Anki export (P4)

- `POST /api/study/export/apkg` builds a real `.apkg` server-side.
- Note fields: Expression, Reading, Glossary, Sentence, SourceBook, Audio.
- Word audio pulled from `audio-server` and embedded as deck media.
- Deck named `Lanobe::{book title}`; `export_log` prevents re-exporting terms.
- At 100% progress the reader offers "Export N new words".
- AnkiConnect stays as an alternate target for desktop.

`.apkg` is the default because it works from a tablet with no Anki running and no
network path back to a desktop.

### 5.6 Offline (P5)

- PWA precaches the app shell; runtime caching for book content and the highlight
  index.
- Writes queue in an outbox and flush on reconnect.
- Conflicts: progress → highest `totalCharsRead` per book wins; saved terms →
  union, with tombstones for deletes.

### 5.7 Auth

Opt-in. No password → open server, with a startup warning. With
`LANOBE_PASSWORD_HASH` set, every route serving library content needs a session.

- argon2id password; HMAC-SHA256 signed cookie carrying only an expiry.
- Constant-time signature comparison.
- Secret generated once and stored in `app.db`, so restarts do not log devices out.
- 90-day sessions — a personal reader opened from a phone should not demand a
  weekly login.
- `/api/app/session`, `/api/app/login` and `/api/system/version` stay public, so a
  client can discover that a password is needed and supply one.

## 6. API surface

| Method | Route | Purpose |
| --- | --- | --- |
| GET | `/api/system/version` | Reachability probe. Public. |
| GET | `/api/app/session` | `{auth_required, authenticated}`. Public. |
| POST | `/api/app/login` / `logout` | Session cookie. Login public. |
| GET/PUT | `/api/app/meta` | Global settings map. |
| GET/POST | `/api/novel/metadata`, `/content/{id}`, `/progress/{id}` | Library and progress. |
| GET/POST/DELETE | `/api/novel/categories`, `/fonts`, `/upload/{id}` | Library management. |
| GET | `/api/yomitan/lookup` | Word lookup. |
| POST | `/api/yomitan/import`, `/install-language`, `/install-defaults` | Dictionaries. |
| GET | `/api/audio` | Word audio. |
| — | `/api/study/*` | Saved vocabulary. **Planned, P3.** |

Unknown `/api/*` returns a JSON 404 rather than falling through to the SPA.

## 7. Configuration

| Flag | Env | Default |
| --- | --- | --- |
| `--host` | `LANOBE_HOST` | `0.0.0.0` |
| `--port` | `LANOBE_PORT` | `4567` |
| `--data-dir` | `LANOBE_DATA_DIR` | platform data dir |
| `--library-path` | `LANOBE_LIBRARY_PATH` | `<data-dir>/library` |
| — | `LANOBE_PASSWORD_HASH` | unset (open) |
| — | `LANOBE_PASSWORD` | unset; hashed at startup, less preferred |
| — | `LANOBE_SECRET` | unset; generated and persisted |

`lanobe hash-password` reads stdin, so the password stays out of shell history.

## 8. Deployment

Built by GitHub Actions, published to `ghcr.io/rifkyhernanda/lanobe`, pulled on
the instance. Publishing is gated on fmt, clippy, tests, lint, build and the smoke
test.

**t2.micro notes**

- 1 GB RAM, no swap by default → **add 2 GB of swap**.
- Steady state: binary 60–100 MB RSS, SQLite page cache capped at 64 MB, Caddy ~20 MB.
- The default 8 GB volume is the real constraint — a full dictionary set runs to
  several GB. **Provision 20–30 GB.**
- **Never import dictionaries on the instance.** Import locally, copy `yomitan.db`
  up. Import is CPU- and RAM-heavy and will burn the burst credits.

## 9. Testing strategy

| Gate | Catches |
| --- | --- |
| `cargo fmt --check` | formatting |
| `cargo clippy --workspace --all-targets -- -D warnings` | lints |
| `cargo test --workspace` | server logic (`--workspace` matters: default-members is the binary alone) |
| `yarn lint` | frontend errors (inherited style debt is warnings) |
| `npx vite build` | bundler resolution |
| `scripts/smoke-test.mjs` | **the app actually rendering** |

The smoke test exists because every other gate passed while the app showed a blank
page: a circular import crashed React before mount. It boots the server, opens
every nav route in headless Chromium, and asserts the page mounts, throws no
uncaught error, reaches `/api/app/session`, and sees no failing API call. It has
since caught two further bugs that would otherwise have shipped.

**Inherited test debt:** 8 deinflector tests fail. Verified against pristine
upstream `45c3bb4` that the identical 8 fail there, so they are not regressions;
marked `#[ignore]`. They are real bugs in the Japanese/Korean/etc. deinflection
rules and may affect lookup quality — worth revisiting during P2.

## 10. Licence

MIT, from Manatan. Parts of `WebUI/src` carry MPL-2.0 headers from Suwayomi and
remain MPL-2.0 — keep the headers, publish modified versions of those files.
