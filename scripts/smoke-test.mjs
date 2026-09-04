/**
 * Boots the built server and asserts the app actually renders.
 *
 * Why this exists: a circular import once left the app showing a blank page while
 * `vite build`, `cargo build`, clippy and every unit test passed. Nothing in the
 * pipeline loads the page, so nothing noticed. This does.
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
const URL = `http://127.0.0.1:${PORT}/`;

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
            const res = await fetch(`${URL}api/system/version`);
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
    const page = await browser.newPage();

    const pageErrors = [];
    const apiResponses = [];
    page.on('pageerror', (e) => pageErrors.push(e.message));
    page.on('response', (r) => {
        const url = r.url();
        if (url.includes('/api/')) apiResponses.push({ url, status: r.status() });
    });

    await page.goto(URL, { waitUntil: 'domcontentloaded', timeout: 30000 });
    // Give React a moment to mount and fire its startup requests.
    await page.waitForTimeout(4000);

    const rendered = await page.evaluate(() => {
        const root = document.getElementById('root');
        return { html: root ? root.innerHTML.length : -1, text: document.body.innerText || '' };
    });

    // 1. The app mounted at all. A TDZ/circular-import crash leaves this at 0.
    if (rendered.html < 500) {
        failures.push(`#root has ${rendered.html} chars of HTML - the app did not render.`);
    }

    // 2. No uncaught exceptions. This is what catches an import cycle.
    if (pageErrors.length) {
        failures.push(`Uncaught page errors:\n  ${pageErrors.join('\n  ')}`);
    }

    // 3. It actually talked to the server. Catches the app deadlocking on its
    //    splash screen with every request stuck in the auth queue.
    const sessionCall = apiResponses.find((r) => r.url.includes('/api/app/session'));
    if (!sessionCall) {
        failures.push(`No /api/app/session request was made. API calls seen: ${
            apiResponses.map((r) => r.url).join(', ') || '(none)'}`);
    } else if (sessionCall.status !== 200) {
        failures.push(`/api/app/session returned ${sessionCall.status}`);
    }

    // 4. No API call failed.
    const failed = apiResponses.filter((r) => r.status >= 400);
    if (failed.length) {
        failures.push(`Failing API calls:\n  ${failed.map((r) => `${r.status} ${r.url}`).join('\n  ')}`);
    }

    if (!failures.length) {
        console.log(`PASS - app rendered (${rendered.html} chars), ${apiResponses.length} API calls, no errors.`);
        console.log(`Visible text: ${rendered.text.replace(/\n+/g, ' / ').slice(0, 160)}`);
    }

    await browser.close();
} catch (e) {
    failures.push(`Smoke test threw: ${e.message}`);
} finally {
    cleanup();
}

if (failures.length) {
    console.error('FAIL\n');
    failures.forEach((f) => console.error(`- ${f}\n`));
    process.exit(1);
}
