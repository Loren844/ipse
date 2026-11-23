// Where each file of the whole memory sits (issue #174): the owner in the middle, each
// kind of file in its own region around them, the most linked files of a region at its
// heart and the others spiralling out. Computed once, in one pass, without a physics
// simulation, so that 20 000 files are laid out at once and always in the same place.
import type { GraphView } from './api.ts';

/** The kinds of the regions, in the order they go round the owner. */
export const REGION_KINDS = ['person', 'place', 'organization', 'project', 'work', 'thing', 'topic', 'event'] as const;

/** Distance between two neighbouring files of a region, in the graph's units. */
export const SPACING = 14;
/** Room kept around the owner. */
const HEART = 90;
/** Room between two regions. */
const GAP = 60;
const GOLDEN = Math.PI * (3 - Math.sqrt(5));

export type Region = {
	kind: string;
	x: number;
	y: number;
	radius: number;
	/** Its files, the most linked first. */
	files: number[];
};

export type Laid = {
	x: Float64Array;
	y: Float64Array;
	/** A file's place in its region, from its heart: 0 for the most linked. */
	rank: Uint32Array;
	/** The links of each file: `links.slice(start[i], start[i + 1])`. */
	start: Uint32Array;
	links: Uint32Array;
	regions: Region[];
	/** The region of each file; -1 for the owner. */
	regionOf: Int16Array;
};

/** The places of the files of `graph`, in the order of `graph.nodes`. */
export function layoutMemory(graph: GraphView): Laid {
	const n = graph.nodes.length;
	const degree = new Uint32Array(n);
	for (let e = 0; e + 1 < graph.edges.length; e += 2) {
		degree[graph.edges[e]!]!++;
		degree[graph.edges[e + 1]!]!++;
	}
	const start = new Uint32Array(n + 1);
	for (let i = 0; i < n; i++) start[i + 1] = start[i]! + degree[i]!;
	const links = new Uint32Array(start[n]!);
	const filled = start.slice(0, n);
	for (let e = 0; e + 1 < graph.edges.length; e += 2) {
		const a = graph.edges[e]!;
		const b = graph.edges[e + 1]!;
		links[filled[a]!++] = b;
		links[filled[b]!++] = a;
	}

	const owner = graph.centered ? 0 : -1;
	const byKind = new Map<string, number[]>();
	graph.nodes.forEach((node, i) => {
		if (i === owner) return;
		const kind = (REGION_KINDS as readonly string[]).includes(node.kind) ? node.kind : 'topic';
		const files = byKind.get(kind) ?? [];
		files.push(i);
		byKind.set(kind, files);
	});
	const regions: Region[] = [];
	for (const kind of REGION_KINDS) {
		const files = byKind.get(kind);
		if (!files?.length) continue;
		// The most linked first; the order of the nodes otherwise (by name, or by date).
		files.sort((a, b) => degree[b]! - degree[a]! || a - b);
		regions.push({ kind, x: 0, y: 0, radius: SPACING * Math.sqrt(files.length) + SPACING, files });
	}

	// Each region gets a share of the turn around the owner by its size, and sits far
	// enough out to stay within its share: no two regions overlap.
	const total = regions.reduce((sum, r) => sum + r.radius, 0);
	let angle = -Math.PI / 2;
	for (const region of regions) {
		const share = regions.length === 1 ? 2 * Math.PI : (2 * Math.PI * region.radius) / total;
		const middle = angle + share / 2;
		const half = Math.min(share / 2, Math.PI / 2);
		const distance = Math.max(HEART + region.radius, (region.radius + GAP / 2) / Math.sin(half));
		region.x = Math.cos(middle) * distance;
		region.y = Math.sin(middle) * distance;
		angle += share;
	}

	const x = new Float64Array(n);
	const y = new Float64Array(n);
	const rank = new Uint32Array(n);
	const regionOf = new Int16Array(n).fill(-1);
	regions.forEach((region, r) => {
		region.files.forEach((file, k) => {
			const radius = SPACING * Math.sqrt(k + 0.5);
			const turn = k * GOLDEN;
			x[file] = region.x + radius * Math.cos(turn);
			y[file] = region.y + radius * Math.sin(turn);
			rank[file] = k;
			regionOf[file] = r;
		});
	});
	return { x, y, rank, start, links, regions, regionOf };
}

/** The files near a point, found without looking at every file. */
export class Grid {
	readonly #cell: number;
	readonly #cells = new Map<string, number[]>();

	constructor(laid: Laid, cell = SPACING * 2) {
		this.#cell = cell;
		for (let i = 0; i < laid.x.length; i++) {
			const key = this.#key(laid.x[i]!, laid.y[i]!);
			const list = this.#cells.get(key);
			if (list) list.push(i);
			else this.#cells.set(key, [i]);
		}
	}

	#key(x: number, y: number) {
		return `${Math.floor(x / this.#cell)},${Math.floor(y / this.#cell)}`;
	}

	/** The nearest file within `reach` of (`x`, `y`) that `shown` keeps; `-1` when none. */
	nearest(laid: Laid, x: number, y: number, reach: number, shown: (i: number) => boolean): number {
		let best = -1;
		let bestDistance = reach * reach;
		const steps = Math.ceil(reach / this.#cell);
		const cx = Math.floor(x / this.#cell);
		const cy = Math.floor(y / this.#cell);
		for (let gx = cx - steps; gx <= cx + steps; gx++) {
			for (let gy = cy - steps; gy <= cy + steps; gy++) {
				for (const i of this.#cells.get(`${gx},${gy}`) ?? []) {
					const dx = laid.x[i]! - x;
					const dy = laid.y[i]! - y;
					const d = dx * dx + dy * dy;
					if (d <= bestDistance && shown(i)) {
						best = i;
						bestDistance = d;
					}
				}
			}
		}
		return best;
	}
}

/** The bounds of the whole graph: `[minX, minY, maxX, maxY]`. */
export function bounds(laid: Laid): [number, number, number, number] {
	let box: [number, number, number, number] = [-HEART, -HEART, HEART, HEART];
	for (const r of laid.regions)
		box = [Math.min(box[0], r.x - r.radius), Math.min(box[1], r.y - r.radius), Math.max(box[2], r.x + r.radius), Math.max(box[3], r.y + r.radius)];
	return box;
}
