import type { Settings } from '@/Manatan/types';

/**
 * Container classes that drive which marks paint, and how.
 *
 * Kept as classes rather than a matcher rebuild: the marks stay in the DOM and
 * just stop painting, so toggling costs a class change instead of re-walking
 * every block -- and it cannot disturb the text offsets reading positions use.
 */
export function highlightClasses(settings: Partial<Settings>): string {
    const classes = [`ln-hl-style-${settings.lnHighlightStyle ?? 'background'}`];
    if (settings.lnHighlightKanji === false) classes.push('ln-hl-no-kanji');
    if (settings.lnHighlightTerms === false) classes.push('ln-hl-no-terms');
    if (settings.lnHighlightOnlyUnknown) classes.push('ln-hl-only-unknown');
    return classes.join(' ');
}
