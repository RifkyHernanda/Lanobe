import { useCallback, useEffect, useRef, useState } from 'react';
import Box from '@mui/material/Box';
import Chip from '@mui/material/Chip';
import CircularProgress from '@mui/material/CircularProgress';
import List from '@mui/material/List';
import ListItem from '@mui/material/ListItem';
import ListItemText from '@mui/material/ListItemText';
import Stack from '@mui/material/Stack';
import Tab from '@mui/material/Tab';
import Tabs from '@mui/material/Tabs';
import Typography from '@mui/material/Typography';
import { useAppTitle } from '@/features/navigation-bar/hooks/useAppTitle.ts';
import { studyApi } from '@/features/study/studyApi.ts';
import type { SavedKanji, SavedTerm } from '@/features/study/Study.types.ts';

type TabKey = 'words' | 'kanji';

const EmptyState = ({ message, hint }: { message: string; hint: string }) => (
    <Stack sx={{ alignItems: 'center', gap: 1, py: 8, px: 2, textAlign: 'center' }}>
        <Typography variant="h6">{message}</Typography>
        <Typography variant="body2" color="text.secondary">
            {hint}
        </Typography>
    </Stack>
);

const WordRow = ({ term }: { term: SavedTerm }) => (
    <ListItem divider>
        <ListItemText
            primary={
                <Stack direction="row" sx={{ alignItems: 'baseline', gap: 1, flexWrap: 'wrap' }}>
                    <Typography component="span" variant="h6">
                        {term.term}
                    </Typography>
                    {!!term.reading && (
                        <Typography component="span" variant="body2" color="text.secondary">
                            {term.reading}
                        </Typography>
                    )}
                    <Chip size="small" label={term.status} />
                </Stack>
            }
            secondary={
                <>
                    {!!term.sentence && (
                        <Typography component="span" variant="body2" display="block">
                            {term.sentence}
                        </Typography>
                    )}
                    {!!term.bookTitle && (
                        <Typography component="span" variant="caption" color="text.secondary">
                            {term.bookTitle}
                        </Typography>
                    )}
                </>
            }
        />
    </ListItem>
);

const KanjiRow = ({ kanji }: { kanji: SavedKanji }) => (
    <ListItem divider>
        <ListItemText
            primary={
                <Stack direction="row" sx={{ alignItems: 'center', gap: 1.5 }}>
                    <Typography component="span" variant="h5">
                        {kanji.char}
                    </Typography>
                    <Chip size="small" label={kanji.status} />
                </Stack>
            }
            secondary={`in ${kanji.termCount} saved ${kanji.termCount === 1 ? 'word' : 'words'}`}
        />
    </ListItem>
);

export const SavedScreen = () => {
    useAppTitle('Saved');

    const [tab, setTab] = useState<TabKey>('words');
    const [terms, setTerms] = useState<SavedTerm[]>([]);
    const [kanji, setKanji] = useState<SavedKanji[]>([]);
    const [loading, setLoading] = useState(true);
    const [error, setError] = useState<string | null>(null);

    // Bumped per request; a response whose generation is stale is dropped. Without
    // this, switching tabs quickly lets the slower request land second and repaint
    // the tab the user already left — the same race that still exists in the
    // lookup popup (see .claude/FOUND-ISSUES.md #5).
    const requestRef = useRef(0);

    const load = useCallback(async (which: TabKey) => {
        const generation = requestRef.current + 1;
        requestRef.current = generation;

        setLoading(true);
        setError(null);
        try {
            if (which === 'words') {
                const result = await studyApi.listTerms({ sort: 'created' });
                if (generation !== requestRef.current) return;
                setTerms(result.items);
            } else {
                const result = await studyApi.listKanji({ sort: 'count' });
                if (generation !== requestRef.current) return;
                setKanji(result.items);
            }
        } catch (e) {
            if (generation !== requestRef.current) return;
            setError(e instanceof Error ? e.message : 'Failed to load saved items');
        } finally {
            if (generation === requestRef.current) {
                setLoading(false);
            }
        }
    }, []);

    useEffect(() => {
        load(tab);
    }, [tab, load]);

    const isEmpty = tab === 'words' ? terms.length === 0 : kanji.length === 0;

    return (
        <Box sx={{ pb: 2 }}>
            <Tabs
                value={tab}
                onChange={(_, next: TabKey) => setTab(next)}
                aria-label="Saved vocabulary"
                sx={{ borderBottom: 1, borderColor: 'divider' }}
            >
                <Tab value="words" label="Words" />
                <Tab value="kanji" label="Kanji" />
            </Tabs>

            {loading && (
                <Stack sx={{ alignItems: 'center', py: 8 }}>
                    <CircularProgress />
                </Stack>
            )}

            {!loading && error && <EmptyState message="Could not load saved items" hint={error} />}

            {!loading && !error && isEmpty && (
                <EmptyState
                    message={tab === 'words' ? 'No saved words yet' : 'No saved kanji yet'}
                    hint="Tap a word while reading and save it from the dictionary popup."
                />
            )}

            {!loading && !error && !isEmpty && (
                <List>
                    {tab === 'words'
                        ? terms.map((term) => <WordRow key={term.id} term={term} />)
                        : kanji.map((entry) => <KanjiRow key={entry.char} kanji={entry} />)}
                </List>
            )}
        </Box>
    );
};
