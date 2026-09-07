import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import Box from '@mui/material/Box';
import Button from '@mui/material/Button';
import Checkbox from '@mui/material/Checkbox';
import Chip from '@mui/material/Chip';
import CircularProgress from '@mui/material/CircularProgress';
import IconButton from '@mui/material/IconButton';
import List from '@mui/material/List';
import ListItem from '@mui/material/ListItem';
import ListItemText from '@mui/material/ListItemText';
import MenuItem from '@mui/material/MenuItem';
import Stack from '@mui/material/Stack';
import Tab from '@mui/material/Tab';
import Tabs from '@mui/material/Tabs';
import TextField from '@mui/material/TextField';
import Typography from '@mui/material/Typography';
import DeleteIcon from '@mui/icons-material/DeleteOutline';
import { useAppTitle } from '@/features/navigation-bar/hooks/useAppTitle.ts';
import { useDebounce } from '@/base/hooks/useDebounce.ts';
import { studyApi } from '@/features/study/studyApi.ts';
import type { LegacyHighlight, SavedKanji, SavedTerm, StudyStats, StudyStatus } from '@/features/study/Study.types.ts';

type TabKey = 'words' | 'kanji' | 'highlights';

const STATUS_FILTERS: { value: StudyStatus | 'all'; label: string }[] = [
    { value: 'all', label: 'All' },
    { value: 'unknown', label: 'Unknown' },
    { value: 'learning', label: 'Learning' },
    { value: 'known', label: 'Known' },
];

const TERM_SORTS = [
    { value: 'created', label: 'Date added' },
    { value: 'frequency', label: 'Frequency' },
    { value: 'term', label: 'Alphabetical' },
] as const;

const KANJI_SORTS = [
    { value: 'count', label: 'Words using it' },
    { value: 'created', label: 'Date added' },
    { value: 'char', label: 'Character' },
] as const;

const EmptyState = ({ message, hint }: { message: string; hint: string }) => (
    <Stack sx={{ alignItems: 'center', gap: 1, py: 8, px: 2, textAlign: 'center' }}>
        <Typography variant="h6">{message}</Typography>
        <Typography variant="body2" color="text.secondary">
            {hint}
        </Typography>
    </Stack>
);

export const SavedScreen = () => {
    useAppTitle('Saved');

    const [tab, setTab] = useState<TabKey>('words');
    const [terms, setTerms] = useState<SavedTerm[]>([]);
    const [kanji, setKanji] = useState<SavedKanji[]>([]);
    const [legacy, setLegacy] = useState<LegacyHighlight[]>([]);
    const [stats, setStats] = useState<StudyStats | null>(null);
    const [loading, setLoading] = useState(true);
    const [error, setError] = useState<string | null>(null);

    const [search, setSearch] = useState('');
    const [book, setBook] = useState('all');
    const [status, setStatus] = useState<StudyStatus | 'all'>('all');
    const [termSort, setTermSort] = useState<(typeof TERM_SORTS)[number]['value']>('created');
    const [kanjiSort, setKanjiSort] = useState<(typeof KANJI_SORTS)[number]['value']>('count');
    const [selected, setSelected] = useState<Set<string>>(new Set());

    // Typing a search term should not fire a request per keystroke over a
    // 60-107ms link; the reader save path is debounced for the same reason.
    const debouncedSearch = useDebounce(search, 300);

    // Bumped per request so a slow response cannot repaint a view the user has
    // already filtered away from.
    const requestRef = useRef(0);

    const load = useCallback(
        async (which: TabKey) => {
            const generation = requestRef.current + 1;
            requestRef.current = generation;

            setLoading(true);
            setError(null);
            try {
                if (which === 'highlights') {
                    const result = await studyApi.listLegacyHighlights(book === 'all' ? undefined : book);
                    if (generation !== requestRef.current) return;
                    setLegacy(result.items);
                } else if (which === 'words') {
                    const result = await studyApi.listTerms({
                        q: debouncedSearch || undefined,
                        book: book === 'all' ? undefined : book,
                        status,
                        sort: termSort,
                    });
                    if (generation !== requestRef.current) return;
                    setTerms(result.items);
                } else {
                    const result = await studyApi.listKanji({
                        q: debouncedSearch || undefined,
                        status,
                        sort: kanjiSort,
                    });
                    if (generation !== requestRef.current) return;
                    setKanji(result.items);
                }
            } catch (e) {
                if (generation !== requestRef.current) return;
                setError(e instanceof Error ? e.message : 'Failed to load saved items');
            } finally {
                if (generation === requestRef.current) setLoading(false);
            }
        },
        [debouncedSearch, book, status, termSort, kanjiSort],
    );

    const refreshStats = useCallback(async () => {
        try {
            setStats(await studyApi.stats());
        } catch {
            setStats(null);
        }
    }, []);

    useEffect(() => {
        load(tab);
    }, [tab, load]);

    useEffect(() => {
        refreshStats();
    }, [refreshStats]);

    // Selections are per-tab and per-filter; keeping them across a change would
    // let a bulk action hit rows the user can no longer see.
    useEffect(() => {
        setSelected(new Set());
    }, [tab, debouncedSearch, book, status, termSort, kanjiSort]);

    const toggle = (key: string) =>
        setSelected((prev) => {
            const next = new Set(prev);
            if (next.has(key)) next.delete(key);
            else next.add(key);
            return next;
        });

    const afterMutation = useCallback(async () => {
        setSelected(new Set());
        await Promise.all([load(tab), refreshStats()]);
    }, [load, tab, refreshStats]);

    const applyBulk = async (action: 'status' | 'delete', next?: StudyStatus) => {
        try {
            if (tab === 'words') {
                await studyApi.bulkTerms([...selected].map(Number), action, next);
            } else if (next) {
                await studyApi.bulkKanji([...selected], next);
            }
            await afterMutation();
        } catch (e) {
            setError(e instanceof Error ? e.message : 'Bulk action failed');
        }
    };

    const removeOne = async (id: number) => {
        try {
            await studyApi.deleteTerm(id);
            await afterMutation();
        } catch (e) {
            setError(e instanceof Error ? e.message : 'Delete failed');
        }
    };

    const setOneStatus = async (key: string, next: StudyStatus) => {
        try {
            if (tab === 'words') await studyApi.setTermStatus(Number(key), next);
            else await studyApi.setKanjiStatus(key, next);
            await Promise.all([load(tab), refreshStats()]);
        } catch (e) {
            setError(e instanceof Error ? e.message : 'Status change failed');
        }
    };

    const books = useMemo(() => stats?.books ?? [], [stats]);
    const isEmpty =
        // eslint-disable-next-line no-nested-ternary
        tab === 'words' ? terms.length === 0 : tab === 'kanji' ? kanji.length === 0 : legacy.length === 0;
    // eslint-disable-next-line no-nested-ternary
    const counts = tab === 'words' ? stats?.terms : tab === 'kanji' ? stats?.kanji : undefined;

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
                <Tab value="highlights" label="Highlights" />
            </Tabs>

            <Stack sx={{ gap: 1.5, p: 2 }}>
                <Stack direction="row" sx={{ gap: 1.5, flexWrap: 'wrap' }}>
                    <TextField
                        size="small"
                        label="Search"
                        value={search}
                        onChange={(e) => setSearch(e.target.value)}
                        sx={{ minWidth: 200, flex: 1 }}
                    />
                    <TextField
                        select
                        size="small"
                        label="Status"
                        value={status}
                        onChange={(e) => setStatus(e.target.value as StudyStatus | 'all')}
                        sx={{ minWidth: 130 }}
                    >
                        {STATUS_FILTERS.map((s) => (
                            <MenuItem key={s.value} value={s.value}>
                                {s.label}
                            </MenuItem>
                        ))}
                    </TextField>
                    {tab !== 'kanji' && (
                        <TextField
                            select
                            size="small"
                            label="Book"
                            value={book}
                            onChange={(e) => setBook(e.target.value)}
                            sx={{ minWidth: 170, maxWidth: 260 }}
                        >
                            <MenuItem value="all">All books</MenuItem>
                            {books.map((b) => (
                                <MenuItem key={b.id} value={b.id}>
                                    {b.title ?? b.id} ({b.count})
                                </MenuItem>
                            ))}
                        </TextField>
                    )}
                    <TextField
                        select
                        size="small"
                        label="Sort"
                        value={tab === 'words' ? termSort : kanjiSort}
                        onChange={(e) =>
                            tab === 'words'
                                ? setTermSort(e.target.value as typeof termSort)
                                : setKanjiSort(e.target.value as typeof kanjiSort)
                        }
                        sx={{ minWidth: 160 }}
                    >
                        {(tab === 'words' ? TERM_SORTS : KANJI_SORTS).map((s) => (
                            <MenuItem key={s.value} value={s.value}>
                                {s.label}
                            </MenuItem>
                        ))}
                    </TextField>
                </Stack>

                {!!counts && (
                    <Typography variant="body2" color="text.secondary">
                        {counts.unknown} unknown · {counts.learning} learning · {counts.known} known
                    </Typography>
                )}

                {selected.size > 0 && (
                    <Stack direction="row" sx={{ gap: 1, alignItems: 'center', flexWrap: 'wrap' }}>
                        <Typography variant="body2">{selected.size} selected</Typography>
                        <Button size="small" onClick={() => applyBulk('status', 'unknown')}>
                            Unknown
                        </Button>
                        <Button size="small" onClick={() => applyBulk('status', 'learning')}>
                            Learning
                        </Button>
                        <Button size="small" onClick={() => applyBulk('status', 'known')}>
                            Known
                        </Button>
                        {tab === 'words' && (
                            <Button size="small" color="error" onClick={() => applyBulk('delete')}>
                                Delete
                            </Button>
                        )}
                        <Button size="small" onClick={() => setSelected(new Set())}>
                            Clear
                        </Button>
                    </Stack>
                )}
            </Stack>

            {loading && (
                <Stack sx={{ alignItems: 'center', py: 8 }}>
                    <CircularProgress />
                </Stack>
            )}

            {!loading && error && <EmptyState message="Could not load saved items" hint={error} />}

            {!loading && !error && isEmpty && (
                <EmptyState
                    message={
                        // eslint-disable-next-line no-nested-ternary
                        tab === 'words' ? 'No saved words' : tab === 'kanji' ? 'No saved kanji' : 'No highlights'
                    }
                    hint={
                        // eslint-disable-next-line no-nested-ternary
                        tab === 'highlights'
                            ? 'Passages you highlighted while reading appear here once the book is opened.'
                            : search || status !== 'all' || book !== 'all'
                              ? 'Nothing matches those filters.'
                              : 'Tap a word while reading and save it from the dictionary popup.'
                    }
                />
            )}

            {!loading && !error && !isEmpty && tab === 'highlights' && (
                <List>
                    {legacy.map((entry) => (
                        <ListItem key={entry.id} divider>
                            <ListItemText
                                primary={<Typography variant="body1">{entry.text}</Typography>}
                                secondary={
                                    <Typography component="span" variant="caption" color="text.secondary">
                                        {entry.bookTitle ?? entry.bookId} · chapter {entry.chapterIndex}
                                        {entry.promotedTermId !== null ? ' · saved as a word' : ''}
                                    </Typography>
                                }
                            />
                        </ListItem>
                    ))}
                </List>
            )}

            {!loading && !error && !isEmpty && tab !== 'highlights' && (
                <List>
                    {tab === 'words'
                        ? terms.map((entry) => (
                              <ListItem
                                  key={entry.id}
                                  divider
                                  secondaryAction={
                                      <IconButton
                                          edge="end"
                                          aria-label={`Delete ${entry.term}`}
                                          onClick={() => removeOne(entry.id)}
                                      >
                                          <DeleteIcon />
                                      </IconButton>
                                  }
                              >
                                  <Checkbox
                                      edge="start"
                                      checked={selected.has(String(entry.id))}
                                      onChange={() => toggle(String(entry.id))}
                                      inputProps={{ 'aria-label': `Select ${entry.term}` }}
                                  />
                                  <ListItemText
                                      primary={
                                          <Stack
                                              direction="row"
                                              sx={{ alignItems: 'baseline', gap: 1, flexWrap: 'wrap' }}
                                          >
                                              <Typography component="span" variant="h6">
                                                  {entry.term}
                                              </Typography>
                                              {!!entry.reading && (
                                                  <Typography component="span" variant="body2" color="text.secondary">
                                                      {entry.reading}
                                                  </Typography>
                                              )}
                                              <Chip
                                                  size="small"
                                                  label={entry.status}
                                                  onClick={() =>
                                                      setOneStatus(
                                                          String(entry.id),
                                                          entry.status === 'known' ? 'unknown' : 'known',
                                                      )
                                                  }
                                              />
                                          </Stack>
                                      }
                                      secondary={
                                          <>
                                              {!!entry.sentence && (
                                                  <Typography component="span" variant="body2" display="block">
                                                      {entry.sentence}
                                                  </Typography>
                                              )}
                                              {!!entry.bookTitle && (
                                                  <Typography component="span" variant="caption" color="text.secondary">
                                                      {entry.bookTitle}
                                                      {entry.chapterIndex !== null
                                                          ? ` · chapter ${entry.chapterIndex}`
                                                          : ''}
                                                  </Typography>
                                              )}
                                          </>
                                      }
                                  />
                              </ListItem>
                          ))
                        : kanji.map((entry) => (
                              <ListItem key={entry.char} divider>
                                  <Checkbox
                                      edge="start"
                                      checked={selected.has(entry.char)}
                                      onChange={() => toggle(entry.char)}
                                      inputProps={{ 'aria-label': `Select ${entry.char}` }}
                                  />
                                  <ListItemText
                                      primary={
                                          <Stack direction="row" sx={{ alignItems: 'center', gap: 1.5 }}>
                                              <Typography component="span" variant="h5">
                                                  {entry.char}
                                              </Typography>
                                              <Chip
                                                  size="small"
                                                  label={entry.status}
                                                  onClick={() =>
                                                      setOneStatus(
                                                          entry.char,
                                                          entry.status === 'known' ? 'unknown' : 'known',
                                                      )
                                                  }
                                              />
                                          </Stack>
                                      }
                                      secondary={`in ${entry.termCount} saved ${
                                          entry.termCount === 1 ? 'word' : 'words'
                                      }`}
                                  />
                              </ListItem>
                          ))}
                </List>
            )}
        </Box>
    );
};
