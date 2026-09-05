import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import { buildMatcher } from '@/features/study/matcher.ts';
import type { HighlightIndex } from '@/features/study/Study.types.ts';

const index = (over: Partial<HighlightIndex> = {}): HighlightIndex => ({
    version: 1,
    kanji: { unknown: '', learning: '' },
    terms: { unknown: [], learning: [] },
    ...over,
});

const keys = (text: string, idx: HighlightIndex) =>
    buildMatcher(idx)
        .match(text)
        .map((h) => h.key);

test('the longest term starting at a position wins', () => {
    const m = buildMatcher(index({ terms: { unknown: ['日本', '日本語'], learning: [] } }));
    assert.deepEqual(
        m.match('日本語を勉強').map((h) => [h.key, h.start, h.end]),
        [['日本語', 0, 3]],
    );
});

test('characters inside a matched term are not also marked as kanji', () => {
    // 語 is saved on its own, but it sits inside 日本語, so it must not produce a
    // second, nested mark.
    const m = buildMatcher(
        index({ terms: { unknown: ['日本語'], learning: [] }, kanji: { unknown: '語', learning: '' } }),
    );
    assert.deepEqual(
        m.match('日本語').map((h) => [h.kind, h.key]),
        [['term', '日本語']],
    );
});

test('kanji outside any term still match', () => {
    const m = buildMatcher(
        index({ terms: { unknown: ['日本'], learning: [] }, kanji: { unknown: '語学', learning: '' } }),
    );
    assert.deepEqual(
        m.match('日本語学').map((h) => [h.kind, h.key]),
        [
            ['term', '日本'],
            ['kanji', '語'],
            ['kanji', '学'],
        ],
    );
});

test('adjacent terms both match', () => {
    assert.deepEqual(keys('図書館学校', index({ terms: { unknown: ['図書館', '学校'], learning: [] } })), [
        '図書館',
        '学校',
    ]);
});

test('hits never overlap', () => {
    const m = buildMatcher(index({ terms: { unknown: ['日本', '日本語', '語学'], learning: [] } }));
    const hits = m.match('日本語学校');
    for (let i = 1; i < hits.length; i += 1) {
        assert.ok(hits[i].start >= hits[i - 1].end, `hit ${i} overlaps the previous one`);
    }
});

test('status is carried through, and learning is distinguished from unknown', () => {
    const m = buildMatcher(
        index({
            terms: { unknown: ['電車'], learning: ['勉強'] },
            kanji: { unknown: '本', learning: '語' },
        }),
    );
    const byKey = Object.fromEntries(m.match('電車勉強本語').map((h) => [h.key, h.status]));
    assert.equal(byKey['電車'], 'unknown');
    assert.equal(byKey['勉強'], 'learning');
    assert.equal(byKey['本'], 'unknown');
    assert.equal(byKey['語'], 'learning');
});

test('surrogate-pair kanji is one hit, not two half-characters', () => {
    // 𠮟 is U+20B9F: two UTF-16 units. Indexing by unit would emit two bogus hits
    // and slice the character in half when the DOM code applies them.
    const m = buildMatcher(index({ kanji: { unknown: '\u{20B9F}', learning: '' } }));
    const hits = m.match('𠮟る');
    assert.equal(hits.length, 1);
    assert.equal(hits[0].key, '\u{20B9F}');
    assert.equal(hits[0].start, 0);
    assert.equal(hits[0].end, 2, 'end is in UTF-16 units so slice() works');
});

test('a term containing a surrogate pair matches', () => {
    const m = buildMatcher(index({ terms: { unknown: ['𠮟る'], learning: [] } }));
    assert.deepEqual(
        m.match('𠮟る').map((h) => [h.key, h.start, h.end]),
        [['𠮟る', 0, 3]],
    );
});

test('an empty index matches nothing and reports itself empty', () => {
    const m = buildMatcher(index());
    assert.equal(m.isEmpty, true);
    assert.deepEqual(m.match('日本語'), []);
});

test('absurdly long saved terms are ignored', () => {
    // Guards the O(n·k) scan: a whole sentence saved by accident would otherwise
    // make every block quadratic.
    const long = 'あ'.repeat(50);
    const m = buildMatcher(index({ terms: { unknown: [long, '日本'], learning: [] } }));
    assert.deepEqual(m.match(long), [], 'the over-long term is not indexed');
    assert.deepEqual(keys('日本', index({ terms: { unknown: [long, '日本'], learning: [] } })), ['日本']);
});

test('a term running past the end of the text does not match', () => {
    assert.deepEqual(keys('日本', index({ terms: { unknown: ['日本語'], learning: [] } })), []);
});

test('text with no matches returns no hits', () => {
    assert.deepEqual(keys('ひらがなだけ', index({ terms: { unknown: ['日本'], learning: [] } })), []);
});

test('offsets index the source string directly', () => {
    const text = 'これは日本語です';
    const m = buildMatcher(index({ terms: { unknown: ['日本語'], learning: [] } }));
    const [hit] = m.match(text);
    assert.equal(text.slice(hit.start, hit.end), '日本語');
});
