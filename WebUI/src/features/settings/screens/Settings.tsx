/*
 * Copyright (C) Contributors to the Suwayomi project
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

import List from '@mui/material/List';
import ListItemIcon from '@mui/material/ListItemIcon';
import ListItemText from '@mui/material/ListItemText';
import PaletteIcon from '@mui/icons-material/Palette';
import InfoIcon from '@mui/icons-material/Info';
import { useTranslation } from 'react-i18next';
import { ListItemLink } from '@/base/components/lists/ListItemLink.tsx';
import { AppRoutes } from '@/base/AppRoute.constants.ts';
import { useAppTitle } from '@/features/navigation-bar/hooks/useAppTitle.ts';

/**
 * Deliberately short.
 *
 * Upstream listed a dozen entries - reader, library, downloads, images, tracking,
 * sync, backup, browse, history, device, server - all of which belonged to the
 * manga/anime halves this fork removed. Linking to routes that no longer exist
 * crashed the page on `.path` of undefined.
 *
 * Everything about reading itself (fonts, writing direction, furigana, dictionary
 * import, Anki targets) lives in the reader's own settings modal, where it can be
 * changed while looking at the page it affects.
 */
export function Settings() {
    const { t } = useTranslation();

    useAppTitle(t('settings.title'));

    return (
        <List sx={{ padding: 0 }}>
            <ListItemLink to={AppRoutes.settings.childRoutes.appearance.path}>
                <ListItemIcon>
                    <PaletteIcon />
                </ListItemIcon>
                <ListItemText primary={t('settings.appearance.title')} />
            </ListItemLink>
            <ListItemLink to={AppRoutes.about.path}>
                <ListItemIcon>
                    <InfoIcon />
                </ListItemIcon>
                <ListItemText primary={t('settings.about.title')} />
            </ListItemLink>
        </List>
    );
}
