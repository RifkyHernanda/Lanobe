import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import { buildMatcher } from '@/features/study/matcher.ts';
import { planMarks } from '@/features/study/markPlan.ts';
import type { HighlightIndex } from '@/features/study/Study.types.ts';

const index = (over: Partial<HighlightIndex> = {}): HighlightIndex => ({
    version: 1,
    kanji: { unknown: '', learning: '' },
    terms: { unknown: [], learning: [] },
    ...over,
});

const segs = (...texts: string[]) => texts.map((text) => ({ text }));
const plan = (texts: string[], idx: HighlightIndex) => planMarks(segs(...texts), buildMatcher(idx));

test('a term inside one segment produces one mark', () => {
    const result = plan(['これは日本語です'], index({ terms: { unknown: ['日本語'], learning: [] } }));
    assert.deepEqual(result, [{ segmentIndex: 0, start: 3, end: 6, kind: 'term', key: '日本語', status: 'unknown' }]);
});

test('SPEC case: a term spanning inline tags produces one mark per segment, sharing a key', () => {
    // 勉<em>強</em>する arrives as three text nodes. Range.surroundContents()
    // throws on a partial non-Text selection, so a single mark is impossible;
    // three marks look identical and never reparent an element boundary.
    const result = plan(['勉', '強', 'する'], index({ terms: { unknown: ['勉強する'], learning: [] } }));

    assert.equal(result.length, 3, 'three segments contribute, so three marks');
    assert.deepEqual(
        result.map((p) => [p.segmentIndex, p.start, p.end]),
        [
            [0, 0, 1],
            [1, 0, 1],
            [2, 0, 2],
        ],
    );
    assert.ok(
        result.every((p) => p.key === '勉強する'),
        'every piece carries the whole term as its key',
    );
});

test('SPEC case: furigana never matches, because rt never becomes a segment', () => {
    // <ruby>漢字<rt>かんじ</rt></ruby>を yields ['漢字', 'を'] -- the walker
    // rejects rt, so the reading is simply absent from the text being matched.
    const withReadingSaved = plan(['漢字', 'を'], index({ terms: { unknown: ['かんじ'], learning: [] } }));
    assert.deepEqual(withReadingSaved, [], 'the reading is not in the text at all');

    const withKanjiSaved = plan(['漢字', 'を'], index({ terms: { unknown: ['漢字'], learning: [] } }));
    assert.deepEqual(
        withKanjiSaved.map((p) => [p.segmentIndex, p.start, p.end, p.key]),
        [[0, 0, 2, '漢字']],
    );
});

test('SPEC case: overlapping saved terms never produce overlapping marks', () => {
    const result = plan(
        ['日本語学校'],
        index({
            terms: { unknown: ['日本', '日本語', '語学'], learning: [] },
            kanji: { unknown: '学校', learning: '' },
        }),
    );

    // 日本語 wins at position 0; 語学 cannot then match because 語 is consumed.
    assert.deepEqual(
        result.map((p) => [p.start, p.end, p.key]),
        [
            [0, 3, '日本語'],
            [3, 4, '学'],
            [4, 5, '校'],
        ],
    );
    for (let i = 1; i < result.length; i += 1) {
        assert.ok(result[i].start >= result[i - 1].end, 'marks must not overlap');
    }
});

test('a term crossing exactly one boundary splits in two', () => {
    const result = plan(['あ日', '本語い'], index({ terms: { unknown: ['日本語'], learning: [] } }));
    assert.deepEqual(
        result.map((p) => [p.segmentIndex, p.start, p.end]),
        [
            [0, 1, 2],
            [1, 0, 2],
        ],
    );
});

test('offsets are local to their segment and slice correctly', () => {
    const segments = segs('これは勉', '強する話');
    const result = planMarks(segments, buildMatcher(index({ terms: { unknown: ['勉強する'], learning: [] } })));
    const pieces = result.map((p) => segments[p.segmentIndex].text.slice(p.start, p.end));
    assert.equal(pieces.join(''), '勉強する');
});

test('empty segments are skipped without breaking offsets', () => {
    const result = plan(['日', '', '本語'], index({ terms: { unknown: ['日本語'], learning: [] } }));
    assert.deepEqual(
        result.map((p) => [p.segmentIndex, p.start, p.end]),
        [
            [0, 0, 1],
            [2, 0, 2],
        ],
    );
});

test('an empty index plans nothing', () => {
    assert.deepEqual(plan(['日本語'], index()), []);
});

test('no segments plans nothing', () => {
    assert.deepEqual(planMarks([], buildMatcher(index({ terms: { unknown: ['日本'], learning: [] } }))), []);
});

test('segments of only whitespace do not shift offsets', () => {
    const result = plan(['  ', '日本語'], index({ terms: { unknown: ['日本語'], learning: [] } }));
    assert.deepEqual(
        result.map((p) => [p.segmentIndex, p.start, p.end]),
        [[1, 0, 3]],
    );
});

test('a surrogate-pair kanji plan slices a whole character', () => {
    const segments = segs('𠮟る');
    const result = planMarks(segments, buildMatcher(index({ kanji: { unknown: '\u{20B9F}', learning: '' } })));
    assert.equal(result.length, 1);
    assert.equal(segments[0].text.slice(result[0].start, result[0].end), '𠮟');
});

test('many segments stay in document order', () => {
    const result = plan(
        ['日', '本', '語', 'を', '勉', '強'],
        index({ terms: { unknown: ['日本語', '勉強'], learning: [] } }),
    );
    assert.deepEqual(
        result.map((p) => p.segmentIndex),
        [0, 1, 2, 4, 5],
    );
});
