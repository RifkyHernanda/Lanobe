import { useEffect, useRef } from 'react';
import { applyToContainer, unmarkContainer } from '@/features/study/applyHighlights.ts';
import type { Matcher } from '@/features/study/matcher.ts';

/** ~2s at 60fps. Long enough for the readers' first content commit. */
const MAX_WAIT_FRAMES = 120;

/**
 * Keeps saved-vocabulary marks applied inside a reader container.
 *
 * The DOM changes from three directions -- ContinuousReader mounting and
 * unmounting chapters, PagedReader swapping whole pages of HTML, and the index
 * itself changing -- so this observes rather than hooking each one.
 */
export function useStudyHighlights(
    containerRef: React.RefObject<HTMLElement | null>,
    matcher: Matcher | undefined,
    etag: string,
    enabled = true,
): void {
    // Set while we are the ones mutating, so the observer does not react to its
    // own marks. takeRecords() drains what we caused.
    const applyingRef = useRef(false);
    const scheduledRef = useRef<number | null>(null);

    useEffect(() => {
        if (!matcher) return undefined;

        let cancelled = false;
        let observer: MutationObserver | null = null;
        let waitFrame = 0;
        let framesWaited = 0;

        const idle: (cb: () => void) => number =
            typeof window.requestIdleCallback === 'function'
                ? (cb) => window.requestIdleCallback(cb, { timeout: 500 })
                : (cb) => window.setTimeout(cb, 0);
        const cancelIdle: (handle: number) => void =
            typeof window.cancelIdleCallback === 'function' ? window.cancelIdleCallback : window.clearTimeout;

        const apply = (container: HTMLElement) => {
            applyingRef.current = true;
            try {
                applyToContainer(container, matcher, etag);
            } finally {
                observer?.takeRecords();
                applyingRef.current = false;
            }
        };

        const setup = () => {
            if (cancelled) return;

            const container = containerRef.current;
            if (!container) {
                // Both readers attach this ref inside a subtree that is absent
                // from the first commit, and a ref populating does not re-render,
                // so no dependency here would ever change to re-run this effect.
                // Waiting a few frames is the difference between working and
                // silently doing nothing forever -- which is exactly what the
                // first version of this hook did.
                if (framesWaited >= MAX_WAIT_FRAMES) return;
                framesWaited += 1;
                waitFrame = requestAnimationFrame(setup);
                return;
            }

            if (!enabled) {
                unmarkContainer(container);
                return;
            }

            observer = new MutationObserver(() => {
                if (applyingRef.current || scheduledRef.current !== null) return;

                scheduledRef.current = idle(() => {
                    scheduledRef.current = null;
                    const current = containerRef.current;
                    if (!cancelled && current) apply(current);
                });
            });

            // First pass, then watch. The per-block stamp makes re-application a
            // no-op, so a redundant pass costs one querySelectorAll.
            apply(container);
            observer.observe(container, { childList: true, subtree: true });
        };

        setup();

        return () => {
            cancelled = true;
            cancelAnimationFrame(waitFrame);
            observer?.disconnect();
            if (scheduledRef.current !== null) cancelIdle(scheduledRef.current);
            scheduledRef.current = null;
        };
    }, [containerRef, matcher, etag, enabled]);
}
