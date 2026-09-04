/*
 * Copyright (C) Contributors to the Suwayomi project
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

import { ThemeMode } from '@/features/theme/AppTheme.types.ts';
import { MetadataServerSettings } from '@/features/settings/Settings.types.ts';

/** Aspect ratio of a book cover in the library grid. */
export const COVER_ASPECT_RATIO = '1 / 1.5';

/** Bounds for the library grid item width slider, in CSS pixels. */
export const MANGA_GRID_WIDTH = {
    min: 100,
    max: 1000,
    step: 10,
    default: 300,
};

export const SERVER_SETTINGS_METADATA_DEFAULT: MetadataServerSettings = {
    // themes
    appTheme: 'default',
    themeMode: ThemeMode.SYSTEM,
    shouldUsePureBlackMode: false,
    customThemes: {},
    mangaThumbnailBackdrop: true,
    mangaDynamicColorSchemes: true,
    mangaGridItemWidth: MANGA_GRID_WIDTH.default,

    // navigation
    hideHistory: false,
};
