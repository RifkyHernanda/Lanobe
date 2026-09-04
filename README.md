# Lanobe

A self-hosted **light novel reader** with instant Japanese dictionary lookup, saved-kanji
tracking, and Anki export. Read on your laptop, phone and tablet against one server.

Lanobe is a light-novel-only fork of [Manatan](https://github.com/KolbyML/Manatan). The
manga, anime, OCR and extension halves are gone, along with the embedded Suwayomi Java
runtime — what remains is a single ~20 MB Rust binary and a React PWA, small enough to run
on a `t2.micro`.

Full specification, plan and UI diagram live in [`.claude/`](.claude/):
[SPEC.md](.claude/SPEC.md) · [PLAN.md](.claude/PLAN.md) · [CLAUDE.md](.claude/CLAUDE.md) · [ui-layout.excalidraw](.claude/ui-layout.excalidraw)

## Status

Under active development. Working today:

- [x] Light-novel-only server (`novel-server`, `yomitan-server`, `audio-server`)
- [x] EPUB library, reader (tategaki, furigana, paged/scroll/virtual), dictionary popup
- [ ] Session auth and server-backed settings (`app-server`)
- [ ] Batched, prefetched, cached dictionary lookups
- [ ] Saved kanji/vocabulary with global auto-highlighting
- [ ] `.apkg` export at the end of a book
- [ ] Offline reading with a write queue
- [ ] Docker image, CI, and the Caddy deployment

## Building

The WebUI is embedded into the binary at compile time, so it has to be built first:

```sh
make build       # yarn build the WebUI, then cargo build --release
./target/release/lanobe
```

Then open <http://127.0.0.1:4567/>.

`make server` alone produces a working binary with a placeholder page instead of the UI —
useful for `cargo check`/`clippy` loops.

### Configuration

| Flag | Environment variable | Default |
| --- | --- | --- |
| `--host` | `LANOBE_HOST` | `0.0.0.0` |
| `--port` | `LANOBE_PORT` | `4567` |
| `--data-dir` | `LANOBE_DATA_DIR` | platform data dir |
| `--library-path` | `LANOBE_LIBRARY_PATH` | `<data-dir>/library` |

Drop `.epub` files into the library directory and they are picked up on startup.

### Dictionaries

Import Yomitan-format dictionaries through the UI. **Import them locally, then copy the
resulting `yomitan.db` to your server** — importing is CPU- and memory-heavy and will
exhaust a small instance's burst credits.

## Migrating from Manatan

The library metadata directory is still called `.manatan-metadata`, so pointing Lanobe at
an existing Manatan novel directory picks up your books and reading progress unchanged.

## Licence

MIT, inherited from Manatan. Parts of the WebUI originate in the
[Suwayomi](https://github.com/Suwayomi/Suwayomi-WebUI) project and carry MPL-2.0 headers;
those files remain under MPL-2.0.
