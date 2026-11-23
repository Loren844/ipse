// A little motion (issue #169): what appears, leaves or changes place moves for 150 to
// 250 ms, never on its own, and not at all when the person asks their system for less
// motion. CSS does most of it (app.css, "Motion"); these are for what Svelte adds and
// removes, which CSS cannot animate on its way out.
import { cubicOut } from 'svelte/easing';
import type { AnimationConfig, FlipParams } from 'svelte/animate';
import { flip } from 'svelte/animate';
import type { TransitionConfig } from 'svelte/transition';

/** `--motion-quick`: small things, a menu, a notice. */
export const QUICK = 150;
/** `--motion-calm`: a line leaving a list, a screen changing. */
export const CALM = 220;

/** True when the person's system asks for less motion: then nothing moves. */
export function still(): boolean {
	return typeof matchMedia === 'function' && matchMedia('(prefers-reduced-motion: reduce)').matches;
}

/** A line leaving a list: it fades and folds up, the lines below closing the gap. */
export function leave(node: Element, { duration = CALM }: { duration?: number } = {}): TransitionConfig {
	if (still()) return { duration: 0 };
	const height = node.getBoundingClientRect().height;
	const style = getComputedStyle(node);
	const margin = parseFloat(style.marginBottom) || 0;
	return {
		duration,
		easing: cubicOut,
		css: (t) => `opacity: ${t}; height: ${t * height}px; margin-bottom: ${t * margin}px; overflow: hidden;`
	};
}

/** Something small that drops in, or lifts out: a menu. */
export function drop(_node: Element, { duration = QUICK }: { duration?: number } = {}): TransitionConfig {
	if (still()) return { duration: 0 };
	return { duration, easing: cubicOut, css: (t) => `opacity: ${t}; transform: translateY(${(1 - t) * 4}px);` };
}

/** Lines of a list sliding to their new place. */
export function move(node: Element, places: { from: DOMRect; to: DOMRect }, params: FlipParams = {}): AnimationConfig {
	return flip(node, places, { duration: still() ? 0 : CALM, easing: cubicOut, ...params });
}
