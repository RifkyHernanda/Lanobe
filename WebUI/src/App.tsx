/*
 * Copyright (C) Contributors to the Suwayomi project
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

import CssBaseline from '@mui/material/CssBaseline';
import React, { useEffect, useLayoutEffect } from 'react';
import { Navigate, Outlet, Route, Routes, useLocation, useNavigate } from 'react-router-dom';
import { loadable } from 'react-lazily/loadable';
import Box from '@mui/material/Box';
import { AwaitableComponent } from 'awaitable-component';
import { AppContext } from '@/base/contexts/AppContext.tsx';
import { DefaultNavBar } from '@/features/navigation-bar/components/DefaultNavBar.tsx';
import { lazyLoadFallback } from '@/base/utils/LazyLoad.tsx';
import { ErrorBoundary } from '@/base/components/feedback/ErrorBoundary.tsx';
import { useNavBarContext } from '@/features/navigation-bar/NavbarContext.tsx';
import { AppRoutes } from '@/base/AppRoute.constants.ts';
import { useNavigationSettings } from '@/features/navigation-bar/NavigationBar.hooks.ts';
import { LoginPage } from '@/features/authentication/screens/LoginPage.tsx';
import { AuthGuard } from '@/features/authentication/components/AuthGuard.tsx';
import { SearchParam } from '@/base/Base.types.ts';
import { ReactRouter } from '@/lib/react-router/ReactRouter.ts';
import { AuthManager } from '@/features/authentication/AuthManager.ts';

// Dictionary lookup context (popup state, Yomitan settings, Anki targets).
import { OCRProvider } from '@/Manatan/context/OCRContext';
import { ManatanHost } from '@/Manatan/ManatanHost';

const { Settings } = loadable(() => import('@/features/settings/screens/Settings.tsx'), lazyLoadFallback);
const { About } = loadable(() => import('@/features/settings/screens/About.tsx'), lazyLoadFallback);
const { Appearance } = loadable(() => import('@/features/settings/screens/Appearance.tsx'), lazyLoadFallback);
const { More } = loadable(() => import('@/features/settings/screens/More.tsx'), lazyLoadFallback);
const { LNLibrary } = loadable(() => import('@/features/ln/screens/LNLibrary.tsx'), lazyLoadFallback);
const { LNReaderScreen } = loadable(() => import('@/features/ln/reader/screens/LNReaderScreen.tsx'), lazyLoadFallback);
const { Dictionary } = loadable(() => import('@/features/dictionary/Dictionary.tsx'), lazyLoadFallback);

const ScrollToTop = () => {
    const { pathname } = useLocation();

    useLayoutEffect(() => {
        window.scrollTo(0, 0);
    }, [pathname]);

    return null;
};

const InitialBackgroundRequests = () => {
    useEffect(() => {
        // Move any LN metadata still held in IndexedDB over to the server.
        import('@/lib/storage/AppStorage').then(({ AppStorage }) => {
            AppStorage.migrateLnMetadata().catch((err) => console.error('[App] LN migration failed:', err));
        });
    }, []);

    return null;
};

const ReactRouterSetter = () => {
    const navigate = useNavigate();

    useEffect(() => {
        ReactRouter.setNavigateFn(navigate);
    }, []);

    return null;
};

const PrivateRoutes = () => {
    const isAuthenticated = AuthManager.useIsAuthenticated();

    if (!isAuthenticated) {
        return (
            <Navigate
                to={{
                    pathname: AppRoutes.authentication.childRoutes.login.path,
                    search: `${SearchParam.REDIRECT}=${window.location.pathname}`,
                }}
                replace
            />
        );
    }

    return <Outlet />;
};

const MainApp = () => {
    const { navBarWidth, appBarHeight, bottomBarHeight } = useNavBarContext();
    const { defaultStartupPage } = useNavigationSettings();

    return (
        <Box
            id="appMainContainer"
            component="main"
            sx={{
                minHeight: `calc(100vh - ${appBarHeight + bottomBarHeight}px)`,
                width: `calc(100vw - (100vw - 100%) - ${navBarWidth}px)`,
                minWidth: `calc(100vw - (100vw - 100%) - ${navBarWidth}px)`,
                maxWidth: `calc(100vw - (100vw - 100%) - ${navBarWidth}px)`,
                position: 'relative',
                mt: `${appBarHeight}px`,
                pb: `calc(${bottomBarHeight}px + ${!bottomBarHeight ? 'env(safe-area-inset-bottom)' : '0px'})`,
                pr: 'env(safe-area-inset-right)',
            }}
        >
            <ErrorBoundary>
                <Routes>
                    <Route path={AppRoutes.authentication.match}>
                        <Route path={AppRoutes.authentication.childRoutes.login.match} element={<LoginPage />} />
                    </Route>

                    <Route element={<PrivateRoutes />}>
                        <Route path={AppRoutes.root.match} element={<Navigate to={defaultStartupPage} replace />} />
                        <Route
                            path={AppRoutes.matchAll.match}
                            element={<Navigate to={AppRoutes.root.path} replace />}
                        />
                        <Route path={AppRoutes.more.match} element={<More />} />
                        <Route path={AppRoutes.about.match} element={<About />} />
                        <Route path={AppRoutes.settings.match}>
                            <Route index element={<Settings />} />
                            <Route path={AppRoutes.settings.childRoutes.appearance.match} element={<Appearance />} />
                        </Route>

                        <Route path={AppRoutes.ln.match} element={<LNLibrary />} />
                        <Route path={AppRoutes.dictionary.match} element={<Dictionary />} />
                    </Route>
                </Routes>
            </ErrorBoundary>
        </Box>
    );
};

const LNReaderApp = () => (
    <ErrorBoundary>
        <Routes>
            <Route element={<PrivateRoutes />}>
                <Route path="*" element={<LNReaderScreen />} />
            </Route>
        </Routes>
    </ErrorBoundary>
);

export const App: React.FC = () => (
    <AppContext>
        <ScrollToTop />
        <AwaitableComponent.Root />

        <AuthGuard>
            <InitialBackgroundRequests />

            <ReactRouterSetter />

            <CssBaseline enableColorScheme />
            <OCRProvider>
                <ManatanHost />
                <Routes>
                    {/* The reader takes the whole viewport - no nav bar. */}
                    <Route
                        path={`${AppRoutes.ln.match}/${AppRoutes.ln.childRoutes.reader.match}/*`}
                        element={<LNReaderApp />}
                    />

                    <Route
                        path="*"
                        element={
                            <Box sx={{ display: 'flex' }}>
                                <Box sx={{ flexShrink: 0, position: 'relative', height: '100vh' }}>
                                    <DefaultNavBar />
                                </Box>
                                <MainApp />
                            </Box>
                        }
                    />
                </Routes>
            </OCRProvider>
        </AuthGuard>
    </AppContext>
);
