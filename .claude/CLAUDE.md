# Lanobe — working notes

Read `SPEC.md` for what this is and `PLAN.md` for what's done and what's next.
This file is the operational stuff: how to build it, and what will bite you.

## Build

The WebUI is embedded into the binary at compile time, so it must be built first.

```sh
make build          # yarn build the WebUI, then cargo build --release
./target/release/lanobe
# → http://127.0.0.1:4567
```

`make server` alone gives a working binary with a placeholder page — fine for
`cargo check` / clippy loops. `build.rs` creates the embed directory if missing,
so a bare `cargo build` never fails on a fresh clone.

Ubuntu prerequisites:

```sh
sudo apt install -y build-essential pkg-config git
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
curl -fsSL https://deb.nodesource.com/setup_22.x | sudo -E bash - && sudo apt install -y nodejs
sudo npm i -g yarn
```

`libsqlite3-sys` uses its **bundled** sqlite, so no system sqlite is needed —
but `ring` and `zstd-sys` still compile C, hence `build-essential`.

## Checks — run all of these before pushing

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace                    # --workspace matters, see below
cd WebUI && yarn lint && npx vite build
node scripts/smoke-test.mjs ./target/release/lanobe
```

The smoke test needs Playwright: `npm install --no-save playwright &&
npx playwright install chromium`. If your environment ships a browser elsewhere,
point `LANOBE_SMOKE_CHROMIUM` at it.

## Traps

**`cargo test` alone tests nothing.** `default-members` is `bin/lanobe`, whose
test binary is empty. Always `--workspace`. CI got this wrong once and reported
green while running zero tests.

**The Rust toolchain is pinned** to 1.95.0 in `.github/workflows/ci.yml` and
`rust:1.95-bookworm` in the Dockerfile. A floating `stable` plus
`clippy -D warnings` means new lints break the build with no commit. If you bump
it, bump both and fix the new lints in that commit. A `rust-toolchain.toml` would
be tidier but breaks any environment that can't reach `static.rust-lang.org`.

**`yarn build` does not typecheck.** It's vite + SWC. ~140 `tsc` errors exist in
inherited code. The real gates are `vite build`, clippy and the smoke test — a
clean `vite build` says nothing about whether the app runs.

**Lint is errors-only.** Inherited Suwayomi/Manatan code carries ~1200 style and
accessibility warnings, downgraded in a documented block in `.eslintrc.cjs`.
`yarn lint` fails on errors. Don't "fix" it by deleting the override; do fix new
errors you introduce.

**Watch for import cycles.** One shipped a blank page: `RestClient` →
`RequestManager` → `new RestClient()`, evaluating the class inside its own
temporal dead zone. It happened because a `import { Type }` was left pointing at
a symbol that no longer existed, so the bundler kept it as a *runtime* edge. If
you delete an export, grep for its importers. `HttpMethod` lives in its own leaf
module for this reason, and `AppStorage` reaches `requestManager` through a
dynamic import.

**8 deinflector tests are `#[ignore]`d.** They fail on upstream `45c3bb4` too, so
they aren't regressions — but they are real bugs in the deinflection rules.
`cargo test -- --ignored` runs them.

## Data

```
<data-dir>/
  app.db          settings + session secret   (SQLite)
  novel.db        library metadata, progress  (sled)
  yomitan.db      dictionaries                (SQLite, can be GBs)
  study.db        saved vocabulary            (SQLite, P3)
  library/        your .epub files
    .manatan-metadata/   covers, parsed content
```

The metadata directory keeps its Manatan name on purpose: point `--library-path`
at an existing Manatan library and books and progress come across untouched.

**Dictionaries.** Nothing ships with one. First run offers to install; otherwise
copy a `yomitan.db` from an existing Manatan install. Never import on the
t2.micro — import locally and copy the file up.

## Layout

```
bin/lanobe/          binary: CLI, router assembly, SPA fallback, embedded WebUI
crates/app-server/   auth + settings          → /api/app
crates/novel-server/ library + progress       → /api/novel
crates/yomitan-server/ lookup + deinflection  → /api/yomitan
crates/audio-server/ word audio               → /api/audio
crates/sync-server/  inherited Google Drive sync; compiled, not routed
WebUI/src/
  features/ln/       the reader and library — the heart of the app
  features/dictionary/ manual search page
  Manatan/           lookup popup, settings modal, setup wizard, Anki client
  lib/requests/      thin REST client
scripts/smoke-test.mjs   boots the server, opens every route in a real browser
```

`src/Manatan/` keeps its name to stay diffable against upstream.

## Conventions

- Comments explain **why**, not what. If a line looks odd, say what would break
  without it.
- Commit messages: what changed, why, and how it was verified. If something is
  unverified, say so — an untested Dockerfile shipped once as if it were tested.
- Don't add a CI gate you haven't run locally.
- When a page misbehaves, reproduce it in the smoke test before fixing it. Two
  bugs were found that way that no unit test could see.
