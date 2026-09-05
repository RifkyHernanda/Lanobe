/**
 * Matches saved kanji and terms against text.
 *
 * Pure: no DOM, no imports beyond types. The DOM half lives in
 * `applyHighlights.ts`, so the interesting semantics here -- overlap, longest
 * match, code-point handling -- can be tested without a browser.
 */

import type { HighlightIndex, StudyStatus } from '@/features/study/Study.types.ts';

export interface Hit {
    /** Index into the source string, in UTF-16 units, so it can slice directly. */
    start: number;
    end: number;
    kind: 'term' | 'kanji';
    /** The matched text: the term, or the single kanji. */
    key: string;
    status: StudyStatus;
}

interface TrieNode {
    next: Map<string, TrieNode>;
    /** Present only on nodes that complete a saved term. */
    end?: StudyStatus;
}

export interface Matcher {
    version: number;
    match(text: string): Hit[];
    /** True when nothing is saved, so callers can skip walking the DOM at all. */
    isEmpty: boolean;
}

/**
 * A term longer than this is not a vocabulary item -- it is a sentence someone
 * saved by accident. The scan is O(n·k) in the longest term, so leaving it
 * unbounded lets one bad row make every block quadratic.
 */
const MAX_TERM_LENGTH = 24;

/** Reads one code point at `i`, so characters above U+FFFF are not split. */
function codePointAt(text: string, i: number): string {
    const cp = text.codePointAt(i);
    return cp === undefined ? text[i] : String.fromCodePoint(cp);
}

function insert(root: TrieNode, term: string, status: StudyStatus): void {
    let node = root;
    for (let i = 0; i < term.length; ) {
        const ch = codePointAt(term, i);
        i += ch.length;
        let child = node.next.get(ch);
        if (!child) {
            child = { next: new Map() };
            node.next.set(ch, child);
        }
        node = child;
    }
    // Only set if absent: `unknown` and `learning` can both contain the same
    // string only through inconsistent data, and first-wins is stable.
    if (node.end === undefined) node.end = status;
}

export function buildMatcher(index: HighlightIndex): Matcher {
    const kanji = new Map<string, StudyStatus>();
    for (const ch of index.kanji.unknown) kanji.set(ch, 'unknown');
    for (const ch of index.kanji.learning) kanji.set(ch, 'learning');

    const root: TrieNode = { next: new Map() };
    let termCount = 0;
    const addAll = (terms: string[], status: StudyStatus) => {
        for (const term of terms) {
            if (!term || term.length > MAX_TERM_LENGTH) continue;
            insert(root, term, status);
            termCount += 1;
        }
    };
    addAll(index.terms.unknown, 'unknown');
    addAll(index.terms.learning, 'learning');

    const isEmpty = kanji.size === 0 && termCount === 0;

    return {
        version: index.version,
        isEmpty,
        match(text: string): Hit[] {
            if (isEmpty || !text) return [];

            const hits: Hit[] = [];
            for (let i = 0; i < text.length; ) {
                // Longest term starting here wins. Walking the trie forward and
                // remembering the last terminal node is what makes 日本語 beat
                // 日本 without needing to sort candidates.
                let node: TrieNode | undefined = root;
                let bestEnd = -1;
                let bestStatus: StudyStatus = 'unknown';

                for (let j = i; j < text.length && node; ) {
                    const ch = codePointAt(text, j);
                    node = node.next.get(ch);
                    if (!node) break;
                    j += ch.length;
                    if (node.end !== undefined) {
                        bestEnd = j;
                        bestStatus = node.end;
                    }
                }

                if (bestEnd > i) {
                    hits.push({
                        start: i,
                        end: bestEnd,
                        kind: 'term',
                        key: text.slice(i, bestEnd),
                        status: bestStatus,
                    });
                    // Skip past the whole term: characters inside a matched word
                    // are not separately marked as kanji, or every term would
                    // carry nested marks.
                    i = bestEnd;
                    continue;
                }

                const ch = codePointAt(text, i);
                const kanjiStatus = kanji.get(ch);
                if (kanjiStatus) {
                    hits.push({ start: i, end: i + ch.length, kind: 'kanji', key: ch, status: kanjiStatus });
                }
                i += ch.length;
            }

            return hits;
        },
    };
}

/** An index-shaped empty value, for before the first fetch resolves. */
export const EMPTY_INDEX: HighlightIndex = {
    version: 0,
    kanji: { unknown: '', learning: '' },
    terms: { unknown: [], learning: [] },
};
