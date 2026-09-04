/*
 * Copyright (C) Contributors to the Suwayomi project
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

import { ReactNode, useMemo, useRef } from 'react';
import { SplashScreen } from '@/features/authentication/components/SplashScreen.tsx';
import { requestManager } from '@/lib/requests/RequestManager.ts';
import { AuthManager } from '@/features/authentication/AuthManager.ts';

/**
 * Marker stored in place of a bearer token. Authentication is a signed HttpOnly
 * cookie the browser sends on its own, so there is no token for the client to
 * hold - but AuthManager models "logged in" as "has a token", and this keeps that
 * bookkeeping intact without spreading cookie awareness through the app.
 */
export const SESSION_MARKER = 'session';

export const AuthGuard = ({ children }: { children: ReactNode }) => {
    const { isAuthRequired } = AuthManager.useSession();
    const [baseUrl, setBaseUrl] = requestManager.useBaseUrl();
    const fallbackAttemptedRef = useRef(false);
    const fallbackBaseUrl = useMemo(() => import.meta.env.VITE_SERVER_URL_DEFAULT, []);

    requestManager.useGetSession({
        skip: isAuthRequired !== null,
        onCompleted: (data) => {
            if (AuthManager.isAuthInitialized()) {
                return;
            }

            AuthManager.setAuthRequired(data.auth_required);

            // An existing cookie means the session is already good; skip the login screen.
            if (data.auth_required && data.authenticated) {
                AuthManager.setTokens(SESSION_MARKER, SESSION_MARKER);
            }

            AuthManager.setAuthInitialized(true);
            requestManager.processQueues();
        },
        onError: (error) => {
            console.warn('[auth] session probe failed', { message: error?.message });

            // A configured server address that no longer resolves would otherwise
            // leave the app stuck on the splash screen forever.
            if (
                !fallbackAttemptedRef.current &&
                fallbackBaseUrl &&
                baseUrl !== fallbackBaseUrl &&
                /status\s+(502|404)|Failed to fetch|Response is not json/i.test(error?.message ?? '')
            ) {
                fallbackAttemptedRef.current = true;
                console.warn('[auth] resetting server base url', { from: baseUrl, to: fallbackBaseUrl });
                setBaseUrl(fallbackBaseUrl);
                requestManager.reset();
            }
        },
    });

    if (isAuthRequired === null) {
        return <SplashScreen />;
    }

    return children;
};
