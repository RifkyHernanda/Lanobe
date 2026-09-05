/**
 * Shapes served by `/api/study/*`.
 *
 * Deliberately a leaf module: it imports nothing at all. Everything else under
 * `features/study/` may depend on it, so if it ever gained an import it could
 * put a cycle underneath the whole feature — and this codebase has shipped a
 * blank page from an import cycle twice.
 */

export type StudyStatus = 'unknown' | 'learning' | 'known';

export interface SavedTerm {
    id: number;
    term: string;
    reading: string;
    surface: string | null;
    gloss: unknown;
    frequency: number | null;
    bookId: string | null;
    bookTitle: string | null;
    chapterIndex: number | null;
    sentence: string;
    createdAt: number;
    status: StudyStatus;
}

export interface SavedKanji {
    char: string;
    /** How many saved words contain this character. */
    termCount: number;
    createdAt: number;
    status: StudyStatus;
}

export interface StatusCounts {
    unknown: number;
    learning: number;
    known: number;
}

export interface StudyStats {
    terms: StatusCounts;
    kanji: StatusCounts;
    books: { id: string; title: string | null; count: number }[];
}

/**
 * `known` entries are absent entirely — that is how "removes it from
 * highlighting without deleting it" is implemented server-side.
 */
export interface HighlightIndex {
    version: number;
    /** Concatenated, not an array: cheaper than per-character JSON quoting. */
    kanji: { unknown: string; learning: string };
    terms: { unknown: string[]; learning: string[] };
}

export interface TermListParams {
    q?: string;
    book?: string;
    status?: StudyStatus | 'all';
    sort?: 'created' | 'frequency' | 'term';
    dir?: 'asc' | 'desc';
    limit?: number;
    offset?: number;
}

export interface KanjiListParams {
    q?: string;
    status?: StudyStatus | 'all';
    sort?: 'created' | 'count' | 'char';
    dir?: 'asc' | 'desc';
    limit?: number;
    offset?: number;
}

export interface NewTermPayload {
    term: string;
    reading?: string;
    surface?: string;
    gloss?: unknown[];
    frequency?: number;
    bookId?: string;
    bookTitle?: string;
    chapterIndex?: number;
    sentence?: string;
    sentenceOffset?: number;
}
