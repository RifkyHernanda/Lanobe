# Found issues

Bugs and defects found while working, with the evidence for each. Kept separate
from `PLAN.md` (which is about phases) so nothing gets lost between sessions.

**Every entry here was verified by reading the code or by measurement — none are
guesses.** When one is fixed, move it to *Fixed* with the commit that did it.

| # | Issue | Severity | Status |
| --- | --- | --- | --- |
| 1 | Furigana corrupts character offsets | **high** | open — fix in P3 S7 |
| 2 | `/saved` has no route, silently redirects | medium | **fixed** |
| 3 | Dictionary-import "loading" state is unreachable | medium | open |
| 4 | `mark.highlight` padding shifts paged layout | medium | open — fix in P3 S7 |
| 5 | A stale lookup can repaint a newer popup | medium | open |
| 6 | Sentence furigana costs ~20 serialised round trips | medium | open |
| 7 | `make check` omits `--workspace` | low | **fixed** |
| 8 | `ci.yml` points at a file that does not exist | low | **fixed** |
| 9 | `SettingsInjector` is dead code | low | open |
| 10 | `rustfmt.toml` sets nightly-only options | low | open |
| 11 | `yomitan-server` pins its own `tower-http` | low | open |
| 12 | 8 deinflector tests fail upstream too | known | `#[ignore]`d |

---

## 1. Furigana corrupts character offsets — **high**

Three places build a `TreeWalker` with a `null` filter, so `<rt>` ruby text is
counted as body text:

- `WebUI/src/features/ln/reader/utils/blockPosition.ts:95` — **produces** the
  saved reading position.
- `WebUI/src/features/ln/reader/utils/restoration.ts:263` — **resolves** an
  offset that was computed from `getCleanTextContent`, which *strips* `rt, rp`
  (`blockPosition.ts:16`).
- `WebUI/src/features/ln/reader/components/PagedReader.tsx:79` —
  `applyHighlightToBlock` resolves highlight start/end offsets.

So the codebase holds two incompatible coordinate spaces — furigana-inclusive and
furigana-exclusive — and the resolvers do not match the producers. In any chapter
with ruby, a restored position lands late by the cumulative length of the furigana
before it. Every light novel has ruby; `義妹生活５` has it on the first page.

`WebUI/src/features/ln/reader/hooks/useTextLookup.ts:111` is the only site that
gets this right, with a walker that rejects `rt, rp`. P3 S7 extracts that walker
to `lib/dom/visibleText.ts` and must repoint all three sites at it, picking one
coordinate space.

> Correction to an earlier note: the mismatch is not *between* `blockPosition`
> and `restoration` — `restoration` imports the clean helper. It is *within*
> `restoration`, between its cleaned offset and its unfiltered walker.

## 2. `/saved` has no route — medium

`AppRoutes.saved` exists (`WebUI/src/base/AppRoute.constants.ts:65`) and the nav
bar links to it, but `WebUI/src/App.tsx` never registers a `<Route>`. It falls
through `AppRoutes.matchAll` to `<Navigate to="/">` and silently shows the
library.

The smoke test reports `ok /saved` because it only asserts `#root` rendered more
than 500 characters — and the library renders. Fixing this needs a real assertion
on the screen's own content, not just that something painted.

## 3. Dictionary-import "loading" state is unreachable — medium

`crates/yomitan-server/src/handlers.rs:1479` returns
`503 {"error":"loading","message":"Dictionaries are importing..."}` while an
import runs. But `WebUI/src/Manatan/utils/api.ts:64` throws on any non-2xx:

```ts
if (!response.ok) { throw new Error(errorMessage); }
```

so the `res.error === 'loading'` check at `api.ts:94` can never run. The throw is
caught by `lookupYomitan`'s own `catch`, which returns `{terms: [], kanji: []}`.
Every `results === 'loading'` branch at all four call sites is dead, and the
`systemLoading` popup state they implement is never shown. During an import you
see "no results" rather than "still importing".

Fix needs `lookupYomitan` to distinguish a 503-with-`loading` from a real failure,
which means not routing it through `apiRequest`'s blanket throw.

## 4. `mark.highlight` padding shifts paged layout — medium

`WebUI/src/features/ln/reader/components/PagedReader.css:631-638` gives marks
`padding: 0 1px` and `border-radius: 2px`. Padding is a layout change, and
`PagedReader` derives its entire page map from `getBoundingClientRect()` during a
measuring pass (`PagedReader.tsx:607-660`). So highlighting text currently moves
page boundaries under the reader.

P3's marks must be strictly layout-neutral — background, `text-decoration` or an
inset `box-shadow` only.

## 5. A stale lookup can repaint a newer popup — medium

`WebUI/src/features/ln/reader/hooks/useTextLookup.ts` has no sequence counter and
no `AbortController` (grepped: zero matches). Tap A, then tap B; if A's response
arrives second, its `setDictPopup(prev => ...)` merges A's results into B's state
— giving B's popup position with A's contents and A's highlight rectangles.

Same shape in `TextBox.tsx:620`, `YomitanPopup.tsx:225/299/370` and
`Dictionary.tsx:106/251/317`.

Fix is a `useRef` generation counter checked after every await, not an
`AbortController` — the guard must also cover the IndexedDB await once a lookup
cache lands.

## 6. Sentence furigana costs ~20 serialised round trips — medium

`WebUI/src/Manatan/utils/japaneseFurigana.ts:303-350`
(`buildSentenceFuriganaFromLookup`) walks a sentence one token at a time,
**awaiting a lookup per token**. Called from `DictionaryView.tsx:750`.

At the measured 60–107 ms RTT to the deploy target, a 40-character sentence is
roughly 20 sequential requests — 1.2–2.0 s. That is larger than any other latency
measured in this project, including the thing P2 was originally aimed at. Needs
cancellation and batching, not just caching.

## 7. `make check` omits `--workspace` — low

`Makefile:20-23` runs `cargo clippy --all-targets` and `cargo test`. This is the
exact trap `CLAUDE.md` documents: `default-members` is `bin/lanobe`, so a bare
`cargo test` runs almost nothing. CI gets it right; the Makefile does not, so
anyone running `make check` locally gets a false green.

## 8. `ci.yml` points at a file that does not exist — low

`.github/workflows/ci.yml:15` says *"Kept in sync with rust-toolchain.toml. See
that file for why this is pinned."* There is no `rust-toolchain.toml` — and
deliberately so: `CLAUDE.md` explains it would break any environment that cannot
reach `static.rust-lang.org`. The comment should point at `CLAUDE.md` instead.

(Confirmed the hard way this session: `rustup` install of 1.95.0 failed once with
a TLS handshake error against exactly that host.)

## 9. `SettingsInjector` is dead code — low

`WebUI/src/Manatan/components/SettingsInjector.tsx` portals into the manga
reader's toolbar, which P0 removed. Either delete it or repoint it at the LN
reader. Carried over from `PLAN.md`'s known issues.

## 10. `rustfmt.toml` sets nightly-only options — low

Every `cargo fmt` prints seven warnings per crate (`wrap_comments`,
`imports_granularity`, `group_imports`, `binop_separator`, …) because they are
unstable and the toolchain is pinned to stable 1.95.0. Formatting still succeeds,
so the options are simply not doing anything. Either drop them or accept the
noise knowingly.

## 11. `yomitan-server` pins its own `tower-http` — low

`crates/yomitan-server/Cargo.toml:31` pins `tower-http = "0.5.2"` instead of
`tower-http.workspace = true` (0.6.8). Both end up compiled into a binary whose
release profile is explicitly tuned for size. Harmless today — `create_router`
returns an erased `axum::Router`, so no `tower-http` type crosses the crate
boundary — but it is duplicated codegen for nothing. Switching it needs `"limit"`
adding to the workspace feature list.

## 12. 8 deinflector tests fail — known, not a regression

Verified against pristine upstream `45c3bb4`: the same 8 fail there, so they are
inherited rather than introduced. Marked `#[ignore]`; `cargo test -- --ignored`
runs them. They are real bugs in the deinflection rules and may be costing lookup
quality.

---

## Fixed

| Issue | Fixed by |
| --- | --- |
| Compose forwarded the host port to a dead container port | `44d6de8` |
| `./data` root-owned by Docker, crash-looping the container | `44d6de8` (documented; the fix is `chown`) |
| Nothing user-visible said "Lanobe" | `ed2986a` |
| `document.title` rendered a bare "- Suwayomi" when `browserTitle` was empty | `ed2986a` |
| No `Cache-Control`/`ETag`: the 897 KB bundle refetched on every load | `a3be8fc` |
| `index.html`'s `<base href>` rewrite reallocated on every SPA navigation | `a3be8fc` |
| No compression: lookups shipped 5–34 KB uncompressed | `71011fd` |
| `bin/lanobe` had an empty test binary | `a3be8fc` |
| #7 `make check` ran `cargo test` without `--workspace`, reporting green while skipping every crate but the binary | S2 |
| #8 `ci.yml` pointed at a non-existent `rust-toolchain.toml` | S2 |
| Re-saving a term erased the book, chapter and glossary captured the first time | S2 (found by running the endpoint; now tested both ways) |
| #2 `/saved` had no `<Route>` and silently redirected to the library | S3 — smoke test now asserts the screen mounts, verified to fail without the fix |
