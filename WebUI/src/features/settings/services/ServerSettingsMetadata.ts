/*
 * Copyright (C) Contributors to the Suwayomi project
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

import { useCallback, useEffect, useMemo, useState, useSyncExternalStore } from 'react';
import { requestManager } from '@/lib/requests/RequestManager.ts';
import { HttpMethod } from '@/lib/requests/client/RestClient.ts';
import { defaultPromiseErrorHandler } from '@/lib/DefaultPromiseErrorHandler.ts';
import { SERVER_SETTINGS_METADATA_DEFAULT } from '@/features/settings/Settings.constants.ts';
import { MetadataServerSettingKeys, MetadataServerSettings } from '@/features/settings/Settings.types.ts';

const META_ENDPOINT = '/api/app/meta';
const LOCAL_CACHE_KEY = 'lanobe_app_meta';

/**
 * Global settings store.
 *
 * The server (`app-server`) is the source of truth so that a setting changed on the
 * laptop shows up on the tablet. localStorage is a cache, not a peer: it lets the UI
 * render with the right theme on the very first paint and keeps the app usable when
 * the server is unreachable (offline, or before app-server exists).
 */
type Listener = () => void;

class AppMetaStore {
    private settings: MetadataServerSettings = AppMetaStore.readLocalCache();

    private listeners = new Set<Listener>();

    private loading = true;

    private error: unknown = undefined;

    private inFlight: Promise<void> | null = null;

    private static readLocalCache(): MetadataServerSettings {
        try {
            const raw = window.localStorage.getItem(LOCAL_CACHE_KEY);
            if (!raw) {
                return { ...SERVER_SETTINGS_METADATA_DEFAULT };
            }
            return { ...SERVER_SETTINGS_METADATA_DEFAULT, ...JSON.parse(raw) };
        } catch {
            return { ...SERVER_SETTINGS_METADATA_DEFAULT };
        }
    }

    private writeLocalCache(): void {
        try {
            window.localStorage.setItem(LOCAL_CACHE_KEY, JSON.stringify(this.settings));
        } catch {
            // Private browsing, quota, or storage disabled - the server copy still stands.
        }
    }

    private emit(): void {
        this.listeners.forEach((listener) => listener());
    }

    subscribe = (listener: Listener): (() => void) => {
        this.listeners.add(listener);
        return () => {
            this.listeners.delete(listener);
        };
    };

    getSnapshot = (): MetadataServerSettings => this.settings;

    getLoading = (): boolean => this.loading;

    getError = (): unknown => this.error;

    /** Fetch from the server, collapsing concurrent callers onto one request. */
    refetch = (): Promise<void> => {
        if (this.inFlight) {
            return this.inFlight;
        }

        this.inFlight = (async () => {
            try {
                const response = await requestManager.getClient().fetcher(META_ENDPOINT);

                if (response.status === 404) {
                    // app-server not present (or not yet deployed): stay on the local cache.
                    this.error = undefined;
                    return;
                }

                const payload = await response.json();
                if (payload && typeof payload === 'object') {
                    this.settings = { ...SERVER_SETTINGS_METADATA_DEFAULT, ...payload };
                    this.writeLocalCache();
                }
                this.error = undefined;
            } catch (e) {
                this.error = e;
            } finally {
                this.loading = false;
                this.inFlight = null;
                this.emit();
            }
        })();

        return this.inFlight;
    };

    update = async <Key extends MetadataServerSettingKeys>(
        key: Key,
        value: MetadataServerSettings[Key],
    ): Promise<void> => {
        // Optimistic: the UI reflects the change immediately, the write follows.
        this.settings = { ...this.settings, [key]: value };
        this.writeLocalCache();
        this.emit();

        await requestManager.getClient().fetcher(META_ENDPOINT, {
            httpMethod: HttpMethod.PUT,
            data: { [key]: value },
        });
    };
}

const store = new AppMetaStore();

export const useMetadataServerSettings = (): {
    settings: MetadataServerSettings;
    loading: boolean;
    request: { loading: boolean; error: unknown; refetch: () => Promise<void> };
} => {
    const settings = useSyncExternalStore(store.subscribe, store.getSnapshot);
    const [, forceRender] = useState(0);

    useEffect(() => {
        store.refetch().finally(() => forceRender((n) => n + 1));
    }, []);

    const loading = store.getLoading();
    const error = store.getError();

    return useMemo(
        () => ({
            settings,
            loading,
            request: { loading, error, refetch: store.refetch },
        }),
        [settings, loading, error],
    );
};

export const getMetadataServerSettings = async (): Promise<MetadataServerSettings> => {
    await store.refetch();
    return store.getSnapshot();
};

export const updateMetadataServerSettings = async <Key extends MetadataServerSettingKeys>(
    setting: Key,
    value: MetadataServerSettings[Key],
): Promise<void> => store.update(setting, value);

export const createUpdateMetadataServerSettings =
    <Key extends MetadataServerSettingKeys>(
        handleError: (error: any) => void = defaultPromiseErrorHandler('createUpdateMetadataServerSettings'),
    ): ((setting: Key, value: MetadataServerSettings[Key]) => Promise<void>) =>
    (setting, value) =>
        updateMetadataServerSettings(setting, value).catch(handleError);

/** Kept for call sites that used to serialise settings into Suwayomi metadata rows. */
export const convertSettingsToMetadata = (settings: Partial<MetadataServerSettings>): Record<string, unknown> => ({
    ...settings,
    customThemes: JSON.stringify(settings.customThemes),
});

export const useUpdateMetadataServerSettings = <Key extends MetadataServerSettingKeys>(): [
    (setting: Key, value: MetadataServerSettings[Key]) => Promise<void>,
] => {
    const update = useCallback(createUpdateMetadataServerSettings<Key>(), []);
    return [update];
};
