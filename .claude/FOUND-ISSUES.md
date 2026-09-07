# Found issues

Bugs and defects found while working, with the evidence for each. Kept separate
from `PLAN.md` (which is about phases) so nothing gets lost between sessions.

**Every entry here was verified by reading the code or by measurement — none are
guesses.** When one is fixed, move it to *Fixed* with the commit that did it.

| # | Issue | Severity | Status |
| --- | --- | --- | --- |
| 1 | Furigana inflated saved reading offsets | **high** | **fixed** (scope narrowed — see below) |
| 2 | `/saved` has no route, silently redirects | medium | **fixed** |
| 3 | Dictionary-import "loading" state is unreachable | medium | **fixed** |
| 4 | `mark.highlight` padding shifts paged layout | medium | **fixed** |
| 5 | A stale lookup can repaint a newer popup | medium | **fixed** (reader path) |
| 6 | Sentence furigana cost one round trip per token | medium | **fixed** |
| 7 | `make check` omits `--workspace` | low | **fixed** |
| 8 | `ci.yml` points at a file that does not exist | low | **fixed** |
| 9 | `SettingsInjector` is dead code | low | open |
| 10 | `rustfmt.toml` sets nightly-only options | low | open |
| 11 | `yomitan-server` pins its own `tower-http` | low | open |
| 12 | 8 deinflector tests fail upstream too | known | `#[ignore]`d |
| 13 | `yarn test` printed a joke and ran nothing | medium | **fixed** |
| 14 | `injectHighlights.ts` does `indexOf` on raw HTML | medium | open |
| 15 | A saved word was not marked until the page reloaded | medium | **fixed** |

---

## 1. Furigana inflated saved reading offsets — **high**, fixed in S7

**The scope was narrower than first recorded, and the mechanism different.** Two
earlier descriptions of this were wrong; this one is traced end to end.

The canonical character space is furigana-**exclusive**: `blockProcessor.ts:277`
builds every block with `getCleanTextContent`, which strips `rt, rp`, and the
resulting `startOffset`/`endOffset` drive progress and restoration.

But `calculatePreciseBlockOffset` (`blockPosition.ts`) produced its local offset
with a `null`-filter walker, counting furigana. `blockMap.ts:154` then does:

```
chapterCharOffset = block.startOffset + min(localOffset, block.endOffset - block.startOffset)
```

— adding a furigana-**inclusive** local offset into a furigana-**exclusive**
global space, and clamping rather than failing. So in a ruby-bearing block the
saved `chapterCharOffset` ran ahead by the length of the readings before the
cursor, silently. That skews progress and makes switching between paged and
continuous jump. `restoration.ts:234`'s ratio fallback divided the same
inclusive offset by a clean length, so it could exceed 1 and snap to the block
end.

Fixed by routing both `calculatePreciseBlockOffset` and `restoration.ts`'s caret
walker through `lib/dom/visibleText.ts`, so producer and resolver agree with the
block map.

**Not part of this bug:** `PagedReader.tsx:79` (`applyHighlightToBlock`). Its
offsets come from `SelectionHandles.tsx:498` via `preRange.toString().length`,
which *includes* ruby text, and it resolves them with an inclusive walker — so
that pair is self-consistent. Changing it would have broken legacy highlights.
They keep their own space until S9 migrates them.

**One-time effect:** positions saved before this fix were inclusive and are now
read as exclusive, so a bookmark in a ruby-heavy block restores slightly early
once. It self-corrects as soon as the position is saved again.

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

Fixed in S8: `lookupYomitan` now does its own fetch and inspects the status and
body, so a `503` carrying `error: "loading"` returns the `'loading'` sentinel and
anything else still throws. The other ~15 `apiRequest` callers are untouched.

## 4. `mark.highlight` padding shifts paged layout — medium

`WebUI/src/features/ln/reader/components/PagedReader.css:631-638` gives marks
`padding: 0 1px` and `border-radius: 2px`. Padding is a layout change, and
`PagedReader` derives its entire page map from `getBoundingClientRect()` during a
measuring pass (`PagedReader.tsx:607-660`). So highlighting text currently moves
page boundaries under the reader.

Fixed in S7: `padding`, `border` and `border-radius` are now zeroed on
`mark.highlight`, and the new `mark.ln-study-mark` is layout-neutral by contract
with a comment in `study.css` saying why nothing else may be added.

## 5. A stale lookup can repaint a newer popup — medium

`WebUI/src/features/ln/reader/hooks/useTextLookup.ts` has no sequence counter and
no `AbortController` (grepped: zero matches). Tap A, then tap B; if A's response
arrives second, its `setDictPopup(prev => ...)` merges A's results into B's state
— giving B's popup position with A's contents and A's highlight rectangles.

Same shape in `TextBox.tsx:620`, `YomitanPopup.tsx:225/299/370` and
`Dictionary.tsx:106/251/317`.

Fixed in S8 for the reader path: `useTextLookup` bumps a `useRef` generation per
tap and checks it before each state write after an await. A counter rather than
an `AbortController` deliberately — it also covers awaits that are not fetches,
which matters once a lookup cache adds an IndexedDB read.

**Still open elsewhere:** `TextBox.tsx:620`, `YomitanPopup.tsx:225/299/370` and
`Dictionary.tsx:106/251/317` have the same shape and are unguarded. The reader is
the path that matters most, but these should get the same treatment.

## 6. Sentence furigana costs ~20 serialised round trips — medium

`WebUI/src/Manatan/utils/japaneseFurigana.ts:303-350`
(`buildSentenceFuriganaFromLookup`) walks a sentence one token at a time,
**awaiting a lookup per token**. Called from `DictionaryView.tsx:750`.

At the measured 60–107 ms RTT this dominated everything else in the app.

Fixed by `POST /api/yomitan/lookup/batch`. The walk is a greedy chain, so its
positions are not known ahead of time — but every character position *can* be
requested at once and the chain then resolved locally from the returned
`matchLen`. Measured on a real sentence (23 characters, 忘れずに a real JMdict):

| | round trips | latency at 60–107 ms RTT |
| --- | --- | --- |
| before | **9** | 540–963 ms |
| after | **1** | 60–107 ms |

The batch response is compact on purpose — headword, reading and `matchLen`
only, no glossary — so all 23 positions cost 1,426 B raw / **379 B gzipped**,
against 5–34 KB for a single full lookup. Some answers go unused; that is much
cheaper than the round trips they replace.

Cancellation is an `isCancelled` callback rather than an `AbortSignal`, so it
also covers the local walk after the response lands, and a `null` batch result
(mid-import or failure) returns the sentence unannotated rather than
half-annotated. Nine unit tests, including one asserting the whole sentence
costs exactly one request.

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

## 14. `injectHighlights.ts` does `indexOf` on raw HTML — medium

`WebUI/src/features/ln/reader/utils/injectHighlights.ts` finds legacy highlight
text by `indexOf` over the raw chapter HTML and splices a `<mark>` in. It can
therefore match inside an attribute value (`alt="…"`, a `src`, a `data-` value)
and inject a tag into it, and it is O(n·m) per chapter.

P3 was going to delete it in S7. It did not, for a reason worth recording: the
new applier works in furigana-**exclusive** offsets, but legacy highlight offsets
are furigana-**inclusive** (`SelectionHandles.tsx:498` uses
`preRange.toString().length`, which includes ruby). Feeding legacy offsets to the
new applier would misplace every one of them.

The fix is a small second applier that reuses the same `splitText` mechanics as
`applyHighlights.ts` but walks with an *inclusive* filter — removing the raw-HTML
splice without changing offset semantics. Deliberately not bundled into the
migration commit, because that would mix "make highlights durable" with "change
how highlights render".

## 15. A saved word was not marked until the page reloaded — medium, fixed

Reported from real use. `useHighlightIndex` fetched once on mount and never
again, so saving a word bumped `index_version` server-side while the client's
matcher stayed stale. The word was genuinely saved — it just was not highlighted
until a reload rebuilt the matcher.

My bug, introduced in S7. Fixed with a module-level listener set
(`notifyHighlightIndexChanged`) that every mounted reader subscribes to, called
by the popup's save control and by the Saved screen's status/delete/bulk paths.
A plain set rather than React context because the writers (the lookup popup) and
the readers (the reader screens) sit in different subtrees and share no
provider.

The refetch deliberately skips `If-None-Match`: after a local write the cached
ETag is exactly what the server would 304 against, so sending it would confirm
the stale copy as current and change nothing.

Verified end to end without a reload: tapped 雑, clicked save, and a mark
appeared within 500 ms.

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
| #13 `yarn test` was `node -e "console.log('imagine')"`, so five .test.ts files had never executed | S6 — now `tsx --test`; all 24 inherited tests passed once actually run |
| #1 furigana-inclusive local offsets mixed into a furigana-exclusive block map | S7 — both walkers now share `lib/dom/visibleText.ts` |
| #4 `mark.highlight` padding shifted PagedReader's measured page boundaries | S7 |
| #3 the import "loading" reply was swallowed by `apiRequest`'s blanket throw | S8 |
| #5 stale lookup responses repainting a newer popup (reader path) | S8 |
| #6 sentence furigana made one round trip per token (9 for a 23-char sentence) | `POST /api/yomitan/lookup/batch` |
| #15 saving a word did not mark it until reload — my own bug from S7, reported from real use | listener + forced refetch |
| Legacy highlights were only in sled, per book, invisible across books | S9 |
| `INSERT OR IGNORE` meant a book title arriving on a later push could never backfill, leaving a raw book id on the Saved screen forever | S9 (found by reading the output, not by assuming) |
| `useStudyHighlights` did nothing at all: it read a ref that is null on the first commit, and no dependency changes when a ref populates | S7 — found by instrumenting the hook after static inspection kept saying the wiring was correct |
