/**
 * Walking the text a reader actually sees.
 *
 * Furigana lives in `<rt>` (and `<rp>` for browsers without ruby support). It is
 * rendered, so `Range.toString()` and a bare `TreeWalker` both include it — but
 * it is *annotation*, not body text. Every character offset in the reader's
 * block map is furigana-exclusive, because `blockProcessor` builds blocks from
 * `getCleanTextContent`, which strips `rt, rp`.
 *
 * So any walk whose offsets feed the block map must reject them too. A leaf
 * module on purpose: `features/ln/` and `features/study/` both depend on it, and
 * a shared helper here keeps that a one-way edge.
 */

/** Text nodes in document order, skipping furigana. */
export function createVisibleTextWalker(root: Node): TreeWalker {
    return document.createTreeWalker(root, NodeFilter.SHOW_TEXT, (node) =>
        node.parentElement?.closest('rt, rp') ? NodeFilter.FILTER_REJECT : NodeFilter.FILTER_ACCEPT,
    );
}

/** Every visible text node under `root`. */
export function collectVisibleTextNodes(root: Node): Text[] {
    const walker = createVisibleTextWalker(root);
    const nodes: Text[] = [];
    let node = walker.nextNode();
    while (node) {
        nodes.push(node as Text);
        node = walker.nextNode();
    }
    return nodes;
}
