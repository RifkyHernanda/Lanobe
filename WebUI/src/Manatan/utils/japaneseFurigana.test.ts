import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import { buildSentenceFuriganaFromLookup } from '@/Manatan/utils/japaneseFurigana';

const encoder = new TextEncoder();

/** Byte offset of each character boundary, matching what the walk sends. */
const byteOffsets = (text: string): number[] => {
    const out: number[] = [];
    let bytes = 0;
    for (const ch of text) {
        out.push(bytes);
        bytes += encoder.encode(ch).length;
    }
    return out;
};

/**
 * A fake batch endpoint built from `{ charIndex: [headword, reading, matchLen] }`,
 * and a counter so the tests can assert the walk makes exactly one request.
 */
const fakeBatch = (text: string, table: Record<number, [string, string, number]>) => {
    const offsets = byteOffsets(text);
    const state = { calls: 0 };
    const lookup = async (t: string, indices: number[]) => {
        state.calls += 1;
        return indices.map((byteIndex) => {
            const charIndex = offsets.indexOf(byteIndex);
            const hit = table[charIndex];
            return {
                index: byteIndex,
                headword: hit?.[0] ?? '',
                reading: hit?.[1] ?? '',
                matchLen: hit?.[2] ?? 0,
            };
        });
    };
    return { lookup, state };
};

test('the whole sentence costs exactly one request', async () => {
    // The point of the change: the walk used to await one lookup per token,
    // which was 9 serialised round trips for a 23-character sentence.
    const text = '俺の知らない';
    const { lookup, state } = fakeBatch(text, {
        0: ['俺', 'おれ', 1],
        2: ['知る', 'しる', 4],
    });

    await buildSentenceFuriganaFromLookup(text, lookup);
    assert.equal(state.calls, 1);
});

test('a match advances by its length and unmatched characters pass through', async () => {
    const text = '俺は学生';
    const { lookup } = fakeBatch(text, {
        0: ['俺', 'おれ', 1],
        2: ['学生', 'がくせい', 2],
    });

    const out = await buildSentenceFuriganaFromLookup(text, lookup);
    assert.equal(out, '<ruby>俺<rt>おれ</rt></ruby>は<ruby>学生<rt>がくせい</rt></ruby>');
});

test('a kana-only match is emitted without ruby', async () => {
    const text = 'ある';
    const { lookup } = fakeBatch(text, { 0: ['ある', 'ある', 2] });

    // Reading equals the text, so annotating it would just be noise.
    assert.equal(await buildSentenceFuriganaFromLookup(text, lookup), 'ある');
});

test('a failed or mid-import batch returns the sentence unannotated', async () => {
    // null rather than [] specifically: half-annotating a sentence is worse
    // than leaving it alone.
    const out = await buildSentenceFuriganaFromLookup('俺の', async () => null);
    assert.equal(out, '俺の');
});

test('cancellation abandons the walk and returns the plain sentence', async () => {
    const text = '俺は学生';
    const { lookup } = fakeBatch(text, { 0: ['俺', 'おれ', 1] });

    const out = await buildSentenceFuriganaFromLookup(text, lookup, { isCancelled: () => true });
    assert.equal(out, text, 'a closed popup should not get annotated markup');
});

test('a non-Japanese language short-circuits without any request', async () => {
    let calls = 0;
    const out = await buildSentenceFuriganaFromLookup(
        'hello',
        async () => {
            calls += 1;
            return [];
        },
        { language: 'english' },
    );
    assert.equal(out, 'hello');
    assert.equal(calls, 0);
});

test('an empty sentence is returned as-is', async () => {
    assert.equal(await buildSentenceFuriganaFromLookup('', async () => []), '');
});

test('a surrogate pair advances a whole code point on a miss', async () => {
    // 𠮟 is two UTF-16 units. Advancing by 1 would emit half a character and
    // then a lone low surrogate.
    const text = '𠮟る';
    const { lookup } = fakeBatch(text, {});

    const out = await buildSentenceFuriganaFromLookup(text, lookup);
    assert.equal(out, text);
    assert.equal([...out].length, 2, 'still two code points, not three units');
});

test('a match reported longer than the remaining text is clamped', async () => {
    const text = '学生';
    const { lookup } = fakeBatch(text, { 0: ['学生活', 'がくせいかつ', 5] });

    // Must not slice past the end or loop forever.
    const out = await buildSentenceFuriganaFromLookup(text, lookup);
    assert.ok(out.includes('学生'), `unexpected output: ${out}`);
});
