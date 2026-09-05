/**
 * Turns matcher hits into per-text-node mark instructions.
 *
 * Pure, and separated from the DOM work on purpose: the three cases SPEC §5.4
 * names -- overlapping terms, terms inside ruby, terms spanning inline tags --
 * are all decided here, so they can be tested without a browser.
 */

import type { Matcher } from '@/features/study/matcher.ts';
import type { StudyStatus } from '@/features/study/Study.types.ts';

/** One text node's contribution, in document order, with `rt`/`rp` excluded. */
export interface Segment {
    text: string;
}

export interface MarkPlan {
    /** Index into the segment list the plan was built from. */
    segmentIndex: number;
    /** Offsets within that segment, in UTF-16 units. */
    start: number;
    end: number;
    kind: 'term' | 'kanji';
    /** The whole matched term, even for a plan covering only part of it. */
    key: string;
    status: StudyStatus;
}

/**
 * Segments are concatenated before matching, which is what lets a term match
 * across an inline element: `勉<em>強</em>する` arrives as three segments and
 * still matches 勉強する. Furigana never reaches here -- the walker that builds
 * the segments rejects `rt` and `rp` -- so a term equal to a reading matches
 * nothing, which is the intent.
 *
 * A hit crossing a segment boundary produces one entry per segment it touches,
 * all sharing a `key`. That is deliberate rather than a compromise:
 * `Range.surroundContents()` throws when a range partially selects a non-Text
 * node, so a single `<mark>` across the boundary cannot be created without
 * reparenting element boundaries. One mark per segment looks identical and
 * never restructures the document.
 */
export function planMarks(segments: Segment[], matcher: Matcher): MarkPlan[] {
    if (matcher.isEmpty || segments.length === 0) return [];

    // Prefix offsets of each segment within the concatenated text.
    const starts: number[] = new Array(segments.length);
    let total = 0;
    for (let i = 0; i < segments.length; i += 1) {
        starts[i] = total;
        total += segments[i].text.length;
    }
    if (total === 0) return [];

    const joined = segments.map((s) => s.text).join('');
    const plans: MarkPlan[] = [];

    let cursor = 0;
    for (const hit of matcher.match(joined)) {
        // Advance a cursor rather than searching: hits are ordered and
        // non-overlapping, so each one starts at or after the previous end.
        while (cursor + 1 < segments.length && starts[cursor + 1] <= hit.start) cursor += 1;

        for (let i = cursor; i < segments.length; i += 1) {
            const segStart = starts[i];
            const segEnd = segStart + segments[i].text.length;
            if (segStart >= hit.end) break;
            if (segEnd <= hit.start) continue;

            const start = Math.max(hit.start, segStart) - segStart;
            const end = Math.min(hit.end, segEnd) - segStart;
            if (end > start) {
                plans.push({
                    segmentIndex: i,
                    start,
                    end,
                    kind: hit.kind,
                    key: hit.key,
                    status: hit.status,
                });
            }
        }
    }

    return plans;
}
