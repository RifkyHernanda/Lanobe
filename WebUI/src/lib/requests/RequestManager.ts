/*
 * Copyright (C) Contributors to the Suwayomi project
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

import { useCallback, useEffect, useRef, useState } from 'react';
import { BaseClient } from '@/lib/requests/client/BaseClient.ts';
import { HttpMethod, RestClient } from '@/lib/requests/client/RestClient.ts';
import { useLocalStorage } from '@/base/hooks/useStorage.tsx';

/**
 * Lanobe talks to its own small REST API, so this is a thin wrapper around
 * {@link RestClient} rather than the ~5k line Suwayomi GraphQL/REST surface it
 * replaced. Anything that needs a route not listed here should call
 * `requestManager.getClient().fetcher(...)` directly.
 */
export class RequestManager {
    public static readonly API_VERSION = '/api/v1/';

    private readonly restClient: RestClient = new RestClient(RequestManager.noTokenRefresh);

    /**
     * Session auth is a cookie, so there is no refresh-token dance to perform. The
     * base client still expects a handler, so hand it one that never refreshes.
     */
    private static noTokenRefresh(): any {
        return {
            response: Promise.resolve({ data: null }),
            abortRequest: () => {},
        };
    }

    public getClient(): RestClient {
        return this.restClient;
    }

    public getBaseUrl(): string {
        return this.restClient.getBaseUrl();
    }

    public useBaseUrl(): ReturnType<typeof useLocalStorage<string>> {
        return useLocalStorage<string>(BaseClient.BASE_URL_KEY, () => this.getBaseUrl());
    }

    public reset(): void {
        this.restClient.reset();
    }

    public processQueues(): void {
        this.restClient.processQueue();
    }

    /**
     * Reachability and session probe in one call.
     *
     * `/api/app/session` is unauthenticated by design: it is how the client learns
     * whether a password is configured at all, which is what lets the splash screen
     * tell "server unreachable" apart from "needs a login".
     */
    public useGetSession({
        skip = false,
        onCompleted,
        onError,
    }: {
        skip?: boolean;
        onCompleted?: (data: { auth_required: boolean; authenticated: boolean }) => void;
        onError?: (error: Error) => void;
    } = {}): void {
        const onCompletedRef = useRef(onCompleted);
        const onErrorRef = useRef(onError);
        onCompletedRef.current = onCompleted;
        onErrorRef.current = onError;

        useEffect(() => {
            if (skip) {
                return;
            }

            let cancelled = false;

            this.restClient
                .fetcher('/api/app/session')
                .then((response) => response.json())
                .then((data) => {
                    if (!cancelled) {
                        onCompletedRef.current?.(data);
                    }
                })
                .catch((error: Error) => {
                    if (!cancelled) {
                        onErrorRef.current?.(error);
                    }
                });

            // eslint-disable-next-line consistent-return
            return () => {
                cancelled = true;
            };
        }, [skip]);
    }

    /**
     * Kept in the shape LoginPage expects. Authentication itself is a signed session
     * cookie set by the server; the returned tokens are placeholders so the existing
     * AuthManager bookkeeping keeps working.
     */
    public useLoginUser(): [
        (options: { variables: { password: string } }) => Promise<{
            data?: { login: { accessToken: string; refreshToken: string } };
        }>,
        { loading: boolean },
    ] {
        const [loading, setLoading] = useState(false);

        const login = useCallback(async ({ variables }: { variables: { password: string } }) => {
            setLoading(true);
            try {
                const response = await this.restClient.fetcher('/api/app/login', {
                    httpMethod: HttpMethod.POST,
                    data: variables,
                });

                if (!response.ok) {
                    throw new Error(`Login failed (${response.status})`);
                }

                return { data: { login: { accessToken: 'session', refreshToken: 'session' } } };
            } finally {
                setLoading(false);
            }
        }, []);

        return [login, { loading }];
    }

    /**
     * Suwayomi's server settings are gone. The dictionary context treats a missing
     * payload as "use local defaults", so returning nothing is the correct behaviour.
     */
    // eslint-disable-next-line class-methods-use-this
    public useGetServerSettings(): { data: undefined } {
        return { data: undefined };
    }

    /** Global settings map, served by app-server. */
    public async getGlobalMeta(): Promise<Record<string, unknown>> {
        const response = await this.restClient.fetcher('/api/app/meta');

        if (response.status === 404) {
            return {};
        }

        return response.json();
    }

    public async setGlobalMetadata(key: string, value: string): Promise<void> {
        await this.restClient.fetcher('/api/app/meta', {
            httpMethod: HttpMethod.PUT,
            data: { [key]: value },
        });
    }
}

export const requestManager = new RequestManager();
