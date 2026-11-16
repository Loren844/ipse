// Where each file of a graph sits: the file at the center in the middle, the files one
// step away on a ring around it, those two steps away on an outer ring, each near the
// file that leads to it. Deterministic, so that the same graph is always drawn the same.
import type { GraphView } from './api.ts';

export const WIDTH = 1000;
export const HEIGHT = 760;
const CX = WIDTH / 2;
const CY = HEIGHT / 2;

export type Spot = { x: number; y: number; angle: number };

/** A spot per node, in the order of `graph.nodes`. */
export function layout(graph: GraphView): Spot[] {
	const spots: Spot[] = graph.nodes.map(() => ({ x: CX, y: CY, angle: 0 }));
	const first = graph.nodes.flatMap((n, i) => (n.depth === 1 ? [i] : []));
	const second = graph.nodes.flatMap((n, i) => (n.depth === 2 ? [i] : []));
	const inner = second.length ? 210 : 260;
	const outer = 340;
	const crowded = first.length > 18;
	first.forEach((node, i) => {
		const angle = -Math.PI / 2 + (2 * Math.PI * i) / first.length;
		// Many files: every other one a little further out, so that they do not touch.
		const radius = crowded && i % 2 ? inner + 44 : inner;
		spots[node] = { x: CX + radius * Math.cos(angle), y: CY + radius * Math.sin(angle) * 0.86, angle };
	});
	// Each file two steps away follows the first file one step away it is linked to.
	const parent = (node: number) => {
		let best = Infinity;
		for (const e of graph.edges) {
			const other = e.from === node ? e.to : e.to === node ? e.from : -1;
			if (other >= 0 && graph.nodes[other]?.depth === 1) best = Math.min(best, first.indexOf(other));
		}
		return best === Infinity ? 0 : best;
	};
	const ordered = second.map((node) => ({ node, parent: parent(node) })).sort((a, b) => a.parent - b.parent || a.node - b.node);
	const shift = first.length ? Math.PI / first.length / 2 : 0;
	ordered.forEach(({ node }, i) => {
		const angle = -Math.PI / 2 - shift + (2 * Math.PI * (i + 0.5)) / ordered.length;
		const radius = ordered.length > 40 && i % 2 ? outer - 30 : outer;
		spots[node] = { x: CX + radius * Math.cos(angle), y: CY + radius * Math.sin(angle) * 0.86, angle };
	});
	return spots;
}

/** Where a label goes beside its node: outward from the center. */
export function labelSide(spot: Spot, depth: number): { dx: number; dy: number; anchor: 'start' | 'middle' | 'end' } {
	if (depth === 0) return { dx: 0, dy: 50, anchor: 'middle' };
	const gap = depth === 1 ? 24 : 16;
	const cos = Math.cos(spot.angle);
	const sin = Math.sin(spot.angle);
	if (cos > 0.35) return { dx: gap, dy: 5, anchor: 'start' };
	if (cos < -0.35) return { dx: -gap, dy: 5, anchor: 'end' };
	return { dx: 0, dy: sin > 0 ? gap + 14 : -gap, anchor: 'middle' };
}
