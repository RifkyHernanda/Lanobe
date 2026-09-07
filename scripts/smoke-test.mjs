/**
 * Boots the built server and asserts every page actually renders.
 *
 * Why this exists, twice over:
 *  - A circular import once left the app blank while `vite build`, `cargo build`,
 *    clippy and every unit test passed. Nothing in the pipeline loaded the page.
 *  - The first version of this script only loaded `/`, and shipped a build where
 *    /settings and /more both crashed. Checking one route proves one route.
 *
 * Usage: node scripts/smoke-test.mjs [path-to-lanobe-binary]
 */
import { chromium } from 'playwright';
import { spawn } from 'node:child_process';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const BINARY = process.argv[2] ?? './target/release/lanobe';
const PORT = 4599;
const ORIGIN = `http://127.0.0.1:${PORT}`;

/** Every route reachable from the navigation bar. */
const ROUTES = ['/', '/ln', '/saved', '/dictionary', '/settings', '/about', '/more'];

const dataDir = mkdtempSync(join(tmpdir(), 'lanobe-smoke-'));
const failures = [];

const server = spawn(BINARY, [
    '--data-dir', dataDir,
    '--library-path', join(dataDir, 'library'),
    '--host', '127.0.0.1',
    '--port', String(PORT),
], { stdio: ['ignore', 'pipe', 'pipe'] });

let serverLog = '';
server.stdout.on('data', (d) => { serverLog += d; });
server.stderr.on('data', (d) => { serverLog += d; });

const cleanup = () => {
    server.kill('SIGTERM');
    rmSync(dataDir, { recursive: true, force: true });
};

/** Poll rather than sleep a fixed amount, so a slow runner does not flake. */
const waitForServer = async () => {
    for (let i = 0; i < 60; i += 1) {
        try {
            const res = await fetch(`${ORIGIN}/api/system/version`);
            if (res.ok) return;
        } catch {
            // not up yet
        }
        await new Promise((r) => setTimeout(r, 500));
    }
    throw new Error(`server did not start within 30s. Log:\n${serverLog}`);
};

/**
 * Seeds one synthetic book plus saved vocabulary, so highlighting can be checked
 * in a real browser against markup chosen to break it. No EPUB fixture needed --
 * novel-server accepts pre-parsed chapter HTML directly.
 *
 * The chapter contains, deliberately:
 *  - a term whose reading is present as furigana (漢字/かんじ), so a mark inside
 *    <rt> would prove the walker is not skipping ruby;
 *  - a term split across an inline element (勉<em>強</em>する), which must yield
 *    one mark per text node rather than one mark reparenting the <em>;
 *  - overlapping saved terms (日本 and 日本語), which must not nest.
 */
const SEED_ID = 'smoke-highlight';
const SEED_BLOCK = 'ch0-b0';
const SEED_HTML =
    `<p data-block-id="${SEED_BLOCK}">` +
    '<ruby>漢字<rt>かんじ</rt></ruby>を勉<em>強</em>する。日本語が好き。' +
    '</p>';

const seedHighlightFixture = async () => {
    const post = async (path, body) => {
        const res = await fetch(`${ORIGIN}${path}`, {
            method: 'POST',
            headers: { 'content-type': 'application/json' },
            body: JSON.stringify(body),
        });
        if (!res.ok) throw new Error(`${path} -> ${res.status} ${await res.text()}`);
    };

    await post(`/api/novel/metadata/${SEED_ID}`, {
        metadata: {
            id: SEED_ID,
            title: 'Smoke Highlight',
            author: '',
            addedAt: 0,
            chapterCount: 1,
            stats: { chapterLengths: [SEED_HTML.length], totalLength: SEED_HTML.length },
            toc: [],
        },
    });
    await post(`/api/novel/content/${SEED_ID}`, {
        chapters: [SEED_HTML],
        imageBlobs: {},
        chapterFilenames: ['c0.xhtml'],
        css: '',
    });

    for (const term of [
        { term: '勉強する', reading: 'べんきょうする' },
        { term: 'かんじ', reading: 'かんじ' },
        { term: '日本', reading: 'にほん' },
        { term: '日本語', reading: 'にほんご' },
    ]) {
        await post('/api/study/terms', term);
    }
};

/** Highlighting is off unless the popup dictionary is enabled. */
const enableLookup = async () => {
    await fetch(`${ORIGIN}/api/app/meta`, {
        method: 'PUT',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ manatan_settings_v1: JSON.stringify({ enableYomitan: true }) }),
    });
};

const checkHighlighting = async (browser) => {
    const context = await browser.newContext({ viewport: { width: 1200, height: 800 } });
    // Otherwise the first-run wizard's overlay covers the reader and swallows it.
    await context.addInitScript(() => localStorage.setItem('manatan_setup_complete_v1', '1'));
    const page = await context.newPage();
    const errors = [];
    page.on('pageerror', (e) => errors.push(e.message));

    await page.goto(`${ORIGIN}/ln/${SEED_ID}/read`, { waitUntil: 'domcontentloaded', timeout: 30000 });
    try {
        await page.waitForSelector('mark.ln-study-mark', { timeout: 20000 });
    } catch {
        failures.push('highlighting: no mark.ln-study-mark appeared in the reader within 20s.');
        await context.close();
        return;
    }

    const result = await page.evaluate((blockId) => {
        const block = document.querySelector(`[data-block-id="${blockId}"]`);
        const marks = [...(block?.querySelectorAll('mark.ln-study-mark') ?? [])];

        // Stripping the marks must restore byte-identical text: every saved
        // reading position and progress figure is an offset into this string.
        const live = block?.textContent ?? '';
        const clone = block?.cloneNode(true);
        clone?.querySelectorAll('mark.ln-study-mark').forEach((m) => {
            const parent = m.parentNode;
            while (m.firstChild) parent.insertBefore(m.firstChild, m);
            parent.removeChild(m);
        });

        return {
            marksInRuby: document.querySelectorAll('rt mark, rp mark').length,
            nested: document.querySelectorAll('mark.ln-study-mark mark.ln-study-mark').length,
            benkyou: marks.filter((m) => m.dataset.lnKey === '勉強する').length,
            nihongo: marks.filter((m) => m.dataset.lnKey === '日本語').length,
            nihon: marks.filter((m) => m.dataset.lnKey === '日本').length,
            textPreserved: (clone?.textContent ?? '') === live,
        };
    }, SEED_BLOCK);

    if (result.marksInRuby > 0) {
        failures.push(`highlighting: ${result.marksInRuby} mark(s) inside <rt> - furigana is being matched.`);
    }
    if (result.nested > 0) {
        failures.push(`highlighting: ${result.nested} nested mark(s) - overlapping hits were emitted.`);
    }
    if (result.benkyou !== 3) {
        failures.push(
            `highlighting: 勉強する spans <em> so it must yield 3 marks, got ${result.benkyou}.`,
        );
    }
    if (result.nihongo !== 1 || result.nihon !== 0) {
        failures.push(
            `highlighting: 日本語 must win over 日本 (expected 1 and 0, got ${result.nihongo} and ${result.nihon}).`,
        );
    }
    if (!result.textPreserved) {
        failures.push('highlighting: removing the marks did not restore the original text - offsets are corrupted.');
    }
    if (errors.length) {
        failures.push(`highlighting: ${errors[0]}`);
    }
    if (!failures.some((f) => f.startsWith('highlighting:'))) {
        console.log('  ok   highlighting (ruby skipped, no nesting, text preserved)');
    }

    await context.close();
};

try {
    await waitForServer();
    await enableLookup();
    await seedHighlightFixture();

    // CI installs its own browser via `npx playwright install chromium`; some
    // environments ship a prebuilt one instead, pointed at by this variable.
    const executablePath = process.env.LANOBE_SMOKE_CHROMIUM || undefined;
    const browser = await chromium.launch(executablePath ? { executablePath } : {});

    for (const route of ROUTES) {
        const page = await browser.newPage();
        const errors = [];
        const apiResponses = [];

        page.on('pageerror', (e) => errors.push(e.message));
        // React's error boundary swallows render errors, so they never reach
        // 'pageerror'. It logs them with this prefix, which is not localised.
        page.on('console', (m) => {
            if (m.type() === 'error' && m.text().includes('Uncaught error')) {
                errors.push(m.text().split('\n')[0]);
            }
        });
        page.on('response', (r) => {
            if (r.url().includes('/api/')) apiResponses.push({ url: r.url(), status: r.status() });
        });

        await page.goto(ORIGIN + route, { waitUntil: 'domcontentloaded', timeout: 30000 });
        // Give React a moment to mount and fire its startup requests.
        await page.waitForTimeout(3000);

        const html = await page.evaluate(() => {
            const root = document.getElementById('root');
            return root ? root.innerHTML.length : -1;
        });

        // A TDZ/circular-import crash leaves this at 0.
        if (html < 500) {
            failures.push(`${route}: #root has ${html} chars - the page did not render.`);
        }
        if (errors.length) {
            failures.push(`${route}: ${errors[0]}`);
        }

        const failed = apiResponses.filter((r) => r.status >= 400);
        if (failed.length) {
            failures.push(`${route}: failing API calls - ${
                failed.map((r) => `${r.status} ${r.url}`).join(', ')}`);
        }

        // `#root` having content is too weak a check for a route that can
        // silently redirect: /saved had no <Route> for the whole of P0-P2, fell
        // through matchAll to the library, and passed this test every time
        // because the library renders. Assert something only this screen shows.
        if (route === '/saved') {
            // Query the element, not innerText: on a fresh data dir the first-run
            // setup wizard covers the page, and innerText only returns *visible*
            // text, so a text match reports a false failure here.
            const tabs = await page.evaluate(() => {
                const tablist = document.querySelector('[aria-label="Saved vocabulary"]');
                if (!tablist) return null;
                return [...tablist.querySelectorAll('[role="tab"]')].map((t) => t.textContent);
            });

            if (tabs === null) {
                failures.push('/saved: SavedScreen did not mount - the route is probably redirecting.');
            } else if (!tabs.includes('Words') || !tabs.includes('Kanji')) {
                failures.push(`/saved: expected Words and Kanji tabs, saw ${JSON.stringify(tabs)}.`);
            }
        }

        // Only the entry route needs to prove the app talks to the server; this
        // catches the app deadlocking with every request stuck in the auth queue.
        if (route === '/') {
            const session = apiResponses.find((r) => r.url.includes('/api/app/session'));
            if (!session) {
                failures.push(`/: no /api/app/session request. Saw: ${
                    apiResponses.map((r) => r.url).join(', ') || '(none)'}`);
            }
        }

        if (!failures.some((f) => f.startsWith(`${route}:`))) {
            console.log(`  ok   ${route}`);
        }

        await page.close();
    }

    await checkHighlighting(browser);

    await browser.close();
} catch (e) {
    failures.push(`Smoke test threw: ${e.message}`);
} finally {
    cleanup();
}

if (failures.length) {
    console.error('\nFAIL\n');
    failures.forEach((f) => console.error(`- ${f}`));
    process.exit(1);
}

console.log(`\nPASS - ${ROUTES.length} routes rendered with no errors.`);
