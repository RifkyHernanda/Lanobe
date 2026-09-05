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

try {
    await waitForServer();

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
