/**
 * Key/value settings storage shared across devices.
 *
 * Upstream stored these in Suwayomi's `meta/global` GraphQL rows. Lanobe serves the
 * same flat map from `/api/app/meta`, so the shape callers see is unchanged.
 */
import { requestManager } from '@/lib/requests/RequestManager.ts';

export const MANATAN_SETTINGS_META_KEY = 'manatan_settings_v1';
export const MANATAN_LN_SETTINGS_META_KEY = 'manatan_ln_settings_by_language_v1';
export const MANATAN_SRS_UI_STATE_META_KEY = 'manatan_srs_ui_state_v1';

export const getServerMetaMap = async (): Promise<Record<string, string>> => {
    const map = await requestManager.getGlobalMeta();

    return Object.fromEntries(
        Object.entries(map ?? {}).map(([key, value]) => [
            key,
            typeof value === 'string' ? value : JSON.stringify(value),
        ]),
    );
};

export const getServerMetaValue = async (key: string): Promise<string | undefined> => {
    const meta = await getServerMetaMap();
    return meta[key];
};

export const setServerMetaValue = async (key: string, value: string): Promise<void> =>
    requestManager.setGlobalMetadata(key, value);

export const getServerMetaJson = async <T>(key: string, fallback: T): Promise<T> => {
    const raw = await getServerMetaValue(key);
    if (!raw) {
        return fallback;
    }

    try {
        return JSON.parse(raw) as T;
    } catch {
        return fallback;
    }
};

export const setServerMetaJson = async <T>(key: string, value: T): Promise<void> =>
    setServerMetaValue(key, JSON.stringify(value));
