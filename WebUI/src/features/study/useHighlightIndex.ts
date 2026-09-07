import { useEffect, useState } from 'react';
import { buildMatcher, EMPTY_INDEX, type Matcher } from '@/features/study/matcher.ts';
import { studyApi } from '@/features/study/studyApi.ts';
import type { HighlightIndex } from '@/features/study/Study.types.ts';

const CACHE_KEY = 'lanobe_highlight_index_v1';

interface Cached {
    etag: string;
    index: HighlightIndex;
}

/**
 * localStorage rather than IndexedDB: the payload is tens of KB at most and the
 * reader wants it synchronously on first paint, before any await, so the
 * previous session's marks do not flash in a frame late.
 */
function readCache(): Cached | null {
    try {
        const raw = localStorage.getItem(CACHE_KEY);
        if (!raw) return null;
        const parsed = JSON.parse(raw) as Cached;
        return parsed?.index && parsed?.etag ? parsed : null;
    } catch {
        return null;
    }
}

function writeCache(value: Cached): void {
    try {
        localStorage.setItem(CACHE_KEY, JSON.stringify(value));
    } catch {
        // Quota or private mode. The index refetches next load; not worth failing over.
    }
}

/**
 * Whether a term is in the locally cached index, without a request.
 *
 * Used by the popup's save control so a word you already saved shows as saved
 * on open. **Caveat:** the index deliberately omits `known` entries, so a word
 * marked known reads as unsaved here. Saving again is harmless -- it upserts and
 * preserves the stored context -- so the cost is a wrong icon, not wrong data.
 * Fixing it properly would need a per-popup request, which is the latency this
 * whole design avoids.
 */
export function isTermInHighlightIndex(term: string): boolean {
    if (!term) return false;
    const cached = readCache();
    if (!cached) return false;
    const { terms } = cached.index;
    return terms.unknown.includes(term) || terms.learning.includes(term);
}

/**
 * Anything that changes saved vocabulary calls this, and every mounted reader
 * refetches the index.
 *
 * Without it, saving a word bumped the server's index version while the
 * client's matcher stayed stale, so a newly saved word did not get marked until
 * the page was reloaded. A module-level set rather than React context because
 * the writers (the lookup popup) and the readers (the reader screens) are in
 * different subtrees and share no provider.
 */
const listeners = new Set<() => void>();

export function notifyHighlightIndexChanged(): void {
    listeners.forEach((listener) => listener());
}

/**
 * The saved-vocabulary index, revalidated with `If-None-Match`.
 *
 * Starts from the cached copy so a reopened book marks immediately, then
 * revalidates: a 304 costs one round trip and no body, which matters because
 * every book open hits this.
 */
export function useHighlightIndex(): { matcher: Matcher; etag: string } {
    const [state, setState] = useState(() => {
        const cached = readCache();
        return {
            matcher: buildMatcher(cached?.index ?? EMPTY_INDEX),
            etag: cached?.etag ?? 'none',
        };
    });

    useEffect(() => {
        let cancelled = false;

        /**
         * @param force Skip `If-None-Match`. After a local write the cached
         *   ETag is exactly the one the server would 304 against, so sending it
         *   would confirm the stale copy is "current" and change nothing.
         */
        const load = async (force = false) => {
            try {
                const cached = readCache();
                const { index, etag } = await studyApi.highlightIndex(force ? undefined : cached?.etag);
                if (cancelled || !index) return;

                const nextEtag = etag ?? `sv${index.version}`;
                writeCache({ etag: nextEtag, index });
                setState({ matcher: buildMatcher(index), etag: nextEtag });
            } catch {
                // Offline, or the server is unreachable. Keep the cached matcher
                // rather than dropping every highlight in the book.
            }
        };

        const onChange = () => {
            void load(true);
        };
        listeners.add(onChange);
        void load();

        return () => {
            cancelled = true;
            listeners.delete(onChange);
        };
    }, []);

    return state;
}
