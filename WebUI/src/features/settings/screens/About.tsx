/*
 * Copyright (C) Contributors to the Suwayomi project
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import List from '@mui/material/List';
import ListItem from '@mui/material/ListItem';
import ListItemText from '@mui/material/ListItemText';
import Link from '@mui/material/Link';
import { useAppTitle } from '@/features/navigation-bar/hooks/useAppTitle.ts';
import { requestManager } from '@/lib/requests/RequestManager.ts';

const REPOSITORY_URL = 'https://github.com/RifkyHernanda/Lanobe';
const UPSTREAM_URL = 'https://github.com/KolbyML/Manatan';

export const About = () => {
    const { t } = useTranslation();
    useAppTitle(t('settings.about.title'));

    const [serverVersion, setServerVersion] = useState<string | null>(null);

    useEffect(() => {
        let cancelled = false;

        requestManager
            .getClient()
            .fetcher('/api/system/version')
            .then((response) => response.json())
            .then((payload) => {
                if (!cancelled) {
                    setServerVersion(payload?.version ?? null);
                }
            })
            .catch(() => {
                if (!cancelled) {
                    setServerVersion(null);
                }
            });

        return () => {
            cancelled = true;
        };
    }, []);

    return (
        <List sx={{ pt: 0 }}>
            <ListItem>
                <ListItemText primary="Lanobe" secondary="Light novel reader with Japanese dictionary lookup" />
            </ListItem>
            <ListItem>
                <ListItemText primary="Server version" secondary={serverVersion ?? 'unavailable'} />
            </ListItem>
            <ListItem>
                <ListItemText
                    primary="Source"
                    secondary={
                        <Link href={REPOSITORY_URL} target="_blank" rel="noreferrer">
                            {REPOSITORY_URL}
                        </Link>
                    }
                />
            </ListItem>
            <ListItem>
                <ListItemText
                    primary="Based on"
                    secondary={
                        <>
                            Manatan by KolbyML (MIT), which builds on the Suwayomi project (MPL-2.0).{' '}
                            <Link href={UPSTREAM_URL} target="_blank" rel="noreferrer">
                                {UPSTREAM_URL}
                            </Link>
                        </>
                    }
                />
            </ListItem>
        </List>
    );
};
