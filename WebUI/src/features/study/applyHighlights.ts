/**
 * Applies mark plans to real DOM.
 *
 * The decisions live in `matcher.ts` and `markPlan.ts`, which are pure and
 * tested. This module is deliberately mechanical: collect text nodes, ask for a
 * plan, wrap. It replaces `injectHighlights.ts`, which did `indexOf` on raw HTML
 * and could therefore inject a `<mark>` into an attribute value.
 */

import { collectVisibleTextNodes } from '@/lib/dom/visibleText.ts';
import { planMarks } from '@/features/study/markPlan.ts';
import type { Matcher } from '@/features/study/matcher.ts';

const MARK_CLASS = 'ln-study-mark';
const MARK_SELECTOR = `mark.${MARK_CLASS}`;
/** Records which index version a block was marked against. */
const STAMP_ATTR = 'data-ln-hl';

/** Undoes `applyToBlock`, restoring the original text-node structure. */
export function unmark(block: Element): void {
    const marks = block.querySelectorAll(MARK_SELECTOR);
    if (marks.length === 0) {
        block.removeAttribute(STAMP_ATTR);
        return;
    }

    marks.forEach((mark) => {
        const parent = mark.parentNode;
        if (!parent) return;
        while (mark.firstChild) parent.insertBefore(mark.firstChild, mark);
        parent.removeChild(mark);
    });

    // Merges the text nodes that splitText created. Without this, repeated
    // apply/unmark cycles would shred a paragraph into hundreds of nodes and
    // every offset walk would get slower for the life of the page.
    block.normalize();
    block.removeAttribute(STAMP_ATTR);
}

/**
 * Marks one block, or does nothing if it already carries this exact index
 * version. The stamp is what makes scrolling and re-renders free.
 */
export function applyToBlock(block: Element, matcher: Matcher, etag: string): void {
    if (block.getAttribute(STAMP_ATTR) === etag) return;

    unmark(block);

    if (matcher.isEmpty) {
        block.setAttribute(STAMP_ATTR, etag);
        return;
    }

    const nodes = collectVisibleTextNodes(block);
    if (nodes.length === 0) {
        block.setAttribute(STAMP_ATTR, etag);
        return;
    }

    const plans = planMarks(
        nodes.map((node) => ({ text: node.data })),
        matcher,
    );

    // Reverse document order. splitText() invalidates offsets in the node being
    // split, so applying the last plan in a node first keeps every earlier
    // offset in that node valid.
    for (let i = plans.length - 1; i >= 0; i -= 1) {
        const plan = plans[i];
        const node = nodes[plan.segmentIndex];
        if (!node.parentNode) continue;

        let target = node;
        if (plan.start > 0) target = target.splitText(plan.start);
        if (plan.end - plan.start < target.data.length) target.splitText(plan.end - plan.start);

        const mark = document.createElement('mark');
        mark.className = MARK_CLASS;
        mark.dataset.lnMark = plan.kind;
        mark.dataset.lnKey = plan.key;
        mark.dataset.lnStatus = plan.status;

        target.replaceWith(mark);
        mark.appendChild(target);
    }

    block.setAttribute(STAMP_ATTR, etag);
}

/** Marks every block under `root`. Returns how many blocks were touched. */
export function applyToContainer(root: ParentNode, matcher: Matcher, etag: string): number {
    const blocks = root.querySelectorAll('[data-block-id]');
    let touched = 0;
    blocks.forEach((block) => {
        if (block.getAttribute(STAMP_ATTR) !== etag) {
            applyToBlock(block, matcher, etag);
            touched += 1;
        }
    });
    return touched;
}

export function unmarkContainer(root: ParentNode): void {
    root.querySelectorAll(`[${STAMP_ATTR}]`).forEach(unmark);
}
