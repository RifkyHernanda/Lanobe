/*
 * Copyright (C) Contributors to the Suwayomi project
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

import { MetadataThemeSettings } from '@/features/theme/AppTheme.types.ts';

/**
 * Global, cross-device app settings.
 *
 * Upstream Manatan derived this from a dozen feature-specific metadata types and
 * persisted it through Suwayomi's `meta/global` GraphQL API. Lanobe keeps only the
 * keys the light-novel UI actually reads, and persists them through
 * `/api/app/meta` (see ServerSettingsMetadata.ts).
 */
export type MetadataServerSettings = MetadataThemeSettings & {
    /** Hide the reading-history entry in the navigation bar. */
    hideHistory: boolean;
};

export type MetadataServerSettingKeys = keyof MetadataServerSettings;

export interface ISearchSettings {
    ignoreFilters: boolean;
}

export type SearchMetadataKeys = keyof ISearchSettings;
