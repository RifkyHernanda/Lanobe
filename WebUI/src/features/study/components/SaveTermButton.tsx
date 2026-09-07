import { useState } from 'react';
import BookmarkAddIcon from '@mui/icons-material/BookmarkAddOutlined';
import BookmarkAddedIcon from '@mui/icons-material/BookmarkAdded';
import CircularProgress from '@mui/material/CircularProgress';
import { useOCR } from '@/Manatan/context/OCRContext';
import { studyApi } from '@/features/study/studyApi.ts';
import { isTermInHighlightIndex, notifyHighlightIndexChanged } from '@/features/study/useHighlightIndex.ts';
import type { DictionaryResult } from '@/Manatan/types';

type SaveState = 'idle' | 'saving' | 'saved' | 'error';

/**
 * Saves the looked-up word to the study database.
 *
 * Distinct from the Anki button beside it: that one pushes a card to a running
 * AnkiConnect instance, this one records vocabulary on the server so it can be
 * highlighted in other books and exported later.
 */
export const SaveTermButton = ({ entry, accentColor }: { entry: DictionaryResult; accentColor?: string }) => {
    const { dictPopup } = useOCR();
    // Seeded from the cached highlight index, so a word you saved earlier shows
    // as saved the moment the popup opens rather than after you click it again.
    // Reads localStorage synchronously -- no request per popup.
    const [state, setState] = useState<SaveState>(() => (isTermInHighlightIndex(entry.headword) ? 'saved' : 'idle'));

    const handleSave = async (event: React.MouseEvent) => {
        event.stopPropagation();
        if (state === 'saving') return;

        setState('saving');
        try {
            const sentence = dictPopup.context?.sentence ?? '';
            const offset = dictPopup.context?.sentenceOffset;
            const source = dictPopup.context?.source;

            // The inflected form as it appeared, which the headword does not
            // preserve: 食べた is stored against the headword 食べる. P4 needs it
            // for cloze sentences. Guarded because matchLen is optional and a
            // bad slice would store nonsense.
            const surface =
                offset !== undefined && entry.matchLen
                    ? sentence.slice(offset, offset + entry.matchLen) || undefined
                    : undefined;

            await studyApi.saveTerm({
                term: entry.headword,
                reading: entry.reading,
                surface,
                gloss: entry.glossary as unknown[],
                bookId: source?.bookId,
                bookTitle: source?.bookTitle,
                chapterIndex: source?.chapterIndex,
                sentence,
                sentenceOffset: offset,
            });
            setState('saved');
            // Rebuild every open reader's matcher, so the word is marked right
            // away instead of only after a reload.
            notifyHighlightIndexChanged();
        } catch (e) {
            console.error('[SaveTermButton] save failed:', e);
            setState('error');
        }
    };

    const label =
        // eslint-disable-next-line no-nested-ternary
        state === 'saved' ? 'Saved to your vocabulary' : state === 'error' ? 'Could not save' : 'Save this word';

    return (
        <button
            type="button"
            onClick={handleSave}
            title={label}
            aria-label={label}
            disabled={state === 'saving'}
            style={{
                background: 'none',
                border: 'none',
                cursor: state === 'saving' ? 'default' : 'pointer',
                padding: '2px',
                color: state === 'error' ? '#ff6b6b' : (accentColor ?? '#7cc8ff'),
                lineHeight: 1,
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'center',
            }}
        >
            {state === 'saving' ? (
                <CircularProgress size={18} color="inherit" />
            ) : state === 'saved' ? (
                <BookmarkAddedIcon sx={{ fontSize: 22 }} />
            ) : (
                <BookmarkAddIcon sx={{ fontSize: 22 }} />
            )}
        </button>
    );
};
