// Where the graph centred on one file puts its neighbours (issue #213): the file in the
// middle, each kind in its own part of the turn around it, the most strongly linked
// nearest. Always the same places for the same neighbours, and no physics: a click
// recentres at once.
import type { NeighbourView } from './api.ts';

/** The kinds a neighbour can be, in the order they go round the center. */
export const NEIGHBOUR_KINDS = ['person', 'place', 'organization', 'project', 'work', 'thing', 'topic'] as const;

/** How each kind of file is drawn: a shape of its own, so that kinds are told apart
 * without a colour code (docs/design/charter.md, section 4). */
export const SHAPES: Record<string, 'circle' | 'square' | 'diamond' | 'triangle' | 'hexagon' | 'down' | 'cross' | 'dot'> = {
	person: 'circle',
	place: 'square',
	organization: 'diamond',
	project: 'triangle',
	work: 'hexagon',
	thing: 'down',
	topic: 'cross',
	event: 'dot'
};

/** Neighbours shown at first, and added by each "Show more". */
export const STEP = 30;

/** Room kept beside the neighbours for their names, in pixels. */
const NAME_ROOM = 150;
/** Height of a neighbour's row, in pixels: names one above the other never touch. */
export const ROW = 26;
/** Room above the first row and below the last, in pixels. */
const EDGE = 24;
/** The nearest a neighbour comes to the center across, as a share of the room. */
const NEAR = 0.45;

export type Place = { x: number; y: number };

/** How strongly a neighbour is linked: a relation written in either file counts as
 * much as five events together (as the core orders them). */
export function strength(n: NeighbourView): number {
	return n.together + (n.words.length || n.back.length ? 5 : 1);
}

/** The height a drawing of `count` neighbours needs, at least `height`. */
export function heightFor(count: number, height: number): number {
	return Math.max(height, Math.ceil(count / 2) * ROW + 2 * EDGE);
}

/** The places of `shown` around the center, by id, in a drawing `width` by `height`
 * pixels whose center is at 0, 0. The neighbours go down the right side, then up the
 * left, by kind, each in a row of its own so that names never overlap, and the most
 * strongly linked nearer the center. */
export function layoutAround(shown: NeighbourView[], width: number, height: number): Map<string, Place> {
	const places = new Map<string, Place>();
	if (!shown.length) return places;
	const strongest = Math.max(...shown.map(strength));
	const weakest = Math.min(...shown.map(strength));
	const order = (kind: string) => {
		const at = (NEIGHBOUR_KINDS as readonly string[]).indexOf(kind);
		return at < 0 ? NEIGHBOUR_KINDS.length : at;
	};
	// By kind, then as the core ordered them (the strongest first).
	const sorted = shown.map((n, i) => ({ n, i })).sort((a, b) => order(a.n.file.kind) - order(b.n.file.kind) || a.i - b.i);
	const right = Math.ceil(sorted.length / 2);
	const across = Math.max(60, width / 2 - NAME_ROOM);
	const down = Math.max(ROW, height / 2 - EDGE);
	sorted.forEach(({ n }, k) => {
		const side = k < right ? 1 : -1;
		const rows = side > 0 ? right : sorted.length - right;
		const row = side > 0 ? k : sorted.length - 1 - k;
		// Rows spread evenly from top to bottom: -1 at the top, 1 at the bottom.
		const v = rows === 1 ? 0 : (2 * row) / (rows - 1) - 1;
		const share = strongest === weakest ? 1 : (strength(n) - weakest) / (strongest - weakest);
		const ring = 1 - (1 - NEAR) * share;
		// On an ellipse, flattened at the top and bottom so the rows there keep their room.
		const bulge = Math.sqrt(Math.max(0, 1 - v * v * 0.85));
		places.set(n.file.id, { x: side * across * ring * bulge, y: v * down * Math.min(1, rows / 6 + 0.2) });
	});
	return places;
}

/** The width of a link's line: 1 for a link without events, up to 6 for the most. */
export function thickness(together: number, most: number): number {
	if (!together || !most) return 1.25;
	return 1.25 + 4.75 * Math.sqrt(together / most);
}
