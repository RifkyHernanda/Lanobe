/**
 * Thin client for `/api/study/*`.
 *
 * `requestManager` is reached through a **dynamic** import. A static one would
 * be provably acyclic today — nothing under `lib/requests/` reaches
 * `features/study/` — but `RequestManager.ts` runs `new RestClient(...)` during
 * module evaluation, and this codebase has twice shipped a blank page from a
 * first-party edge into that cluster resolving in the wrong order. `AppStorage`
 * defers the same import for the same reason. Every caller here is already
 * async, so it costs nothing.
 */

import type {
    HighlightIndex,
    KanjiListParams,
    NewTermPayload,
    SavedKanji,
    SavedTerm,
    StudyStats,
    StudyStatus,
} from '@/features/study/Study.types.ts';

const getClient = async () => (await import('@/lib/requests/RequestManager.ts')).requestManager.getClient();

function toQueryString(params: Record<string, unknown>): string {
    const search = new URLSearchParams();
    Object.entries(params).forEach(([key, value]) => {
        // 'all' is the UI's word for "no filter"; the server wants the param absent.
        if (value === undefined || value === null || value === '' || value === 'all') return;
        search.set(key, String(value));
    });
    const query = search.toString();
    return query ? `?${query}` : '';
}

async function getJson<T>(path: string): Promise<T> {
    const client = await getClient();
    const response = await client.fetcher(path);
    return response.json();
}

export interface TermListResult {
    items: SavedTerm[];
    total: number;
}

export interface KanjiListResult {
    items: SavedKanji[];
    total: number;
}

export const studyApi = {
    async listTerms(params: TermListParams = {}): Promise<TermListResult> {
        return getJson(`/api/study/terms${toQueryString(params as Record<string, unknown>)}`);
    },

    async listKanji(params: KanjiListParams = {}): Promise<KanjiListResult> {
        return getJson(`/api/study/kanji${toQueryString(params as Record<string, unknown>)}`);
    },

    async stats(): Promise<StudyStats> {
        return getJson('/api/study/stats');
    },

    async saveTerm(payload: NewTermPayload): Promise<{ id: number; created: boolean; indexVersion: number }> {
        const client = await getClient();
        const response = await client.post('/api/study/terms', payload);
        return response.json();
    },

    async setTermStatus(id: number, status: StudyStatus): Promise<{ indexVersion: number }> {
        const client = await getClient();
        const response = await client.patch(`/api/study/terms/${id}`, { status });
        return response.json();
    },

    async setKanjiStatus(char: string, status: StudyStatus): Promise<{ indexVersion: number }> {
        const client = await getClient();
        // The character goes in the body, not the path: percent-encoding a
        // 4-byte CJK Extension B character into a URL is a good way to have it
        // survive the router and die in a proxy.
        const response = await client.patch('/api/study/kanji', { char, status });
        return response.json();
    },

    async deleteTerm(id: number): Promise<{ indexVersion: number }> {
        const client = await getClient();
        const response = await client.delete(`/api/study/terms/${id}`);
        return response.json();
    },

    /**
     * One request for the whole selection, not one per row: the server bumps the
     * highlight-index version once per call, so a per-row loop would invalidate
     * every open reader's cached index N times for a single user action.
     */
    async bulkTerms(
        ids: number[],
        action: 'status' | 'delete',
        status?: StudyStatus,
    ): Promise<{ affected: number; indexVersion: number }> {
        const client = await getClient();
        const response = await client.post('/api/study/terms/bulk', { ids, action, status });
        return response.json();
    },

    async bulkKanji(chars: string[], status: StudyStatus): Promise<{ affected: number; indexVersion: number }> {
        const client = await getClient();
        const response = await client.post('/api/study/kanji/bulk', { chars, status });
        return response.json();
    },

    /**
     * Revalidates with `If-None-Match`. Returns `null` when the server answers
     * 304, meaning the caller's cached copy is still current.
     */
    async highlightIndex(etag?: string): Promise<{ index: HighlightIndex | null; etag: string | null }> {
        const client = await getClient();
        const response = await client.fetcher('/api/study/highlight-index', {
            config: etag ? { headers: { 'If-None-Match': etag } } : undefined,
            allowedStatuses: [304],
        });

        if (response.status === 304) {
            return { index: null, etag: etag ?? null };
        }

        return { index: await response.json(), etag: response.headers.get('etag') };
    },
};
