/*
 * Copyright (C) Contributors to the Suwayomi project
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

type AppRouteInfo = {
    match: string;
    path?: string | ((...args: any[]) => string);
};

type TAppRoutes = Record<string, AppRouteInfo & { childRoutes?: TAppRoutes }>;

export const AppRoutes = {
    root: {
        match: '/',
        path: '/',
    },
    matchAll: {
        match: '*',
    },
    authentication: {
        match: 'auth',
        path: '/auth',
        childRoutes: {
            login: {
                match: 'login',
                path: '/auth/login',
            },
        },
    },
    about: {
        match: 'about',
        path: '/about',
    },
    more: {
        match: '/more',
        path: '/more',
    },
    settings: {
        match: 'settings',
        path: '/settings',
        childRoutes: {
            appearance: {
                match: 'appearance',
                path: '/settings/appearance',
            },
            categories: {
                match: 'categories',
                path: '/settings/categories',
            },
            server: {
                match: 'server',
                path: '/settings/server',
            },
        },
    },
    dictionary: {
        match: 'dictionary',
        path: '/dictionary',
    },
    /** Saved kanji and vocabulary (see study-server). */
    saved: {
        match: 'saved',
        path: '/saved',
    },
    ln: {
        match: 'ln',
        path: '/ln',
        childRoutes: {
            reader: {
                match: ':id/read',
                path: (id: string) => `/ln/${id}/read`,
            },
        },
    },
} as const satisfies TAppRoutes;

type ExtractChildRouteStringPaths<T> = T extends { childRoutes: infer U } ? ExtractStringPaths<U[keyof U]> : never;

type ExtractStringPaths<T> = T extends { path: infer P }
    ? P extends string
        ? P | ExtractChildRouteStringPaths<T>
        : ExtractChildRouteStringPaths<T>
    : ExtractChildRouteStringPaths<T>;

export type StaticAppRoute = ExtractStringPaths<(typeof AppRoutes)[keyof typeof AppRoutes]>;
