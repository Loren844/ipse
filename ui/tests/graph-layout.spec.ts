// Where the whole memory's graph puts each file (issue #174), without a browser: the
// owner in the middle, a region per kind that never overlaps another, the most linked
// files at a region's heart, always the same places. Data invented.
import { expect, test } from '@playwright/test';
import type { GraphView } from '../src/lib/api.ts';
import { Grid, layoutMemory } from '../src/lib/memoryGraph.ts';

function memory(people: number, places: number, events: number): GraphView {
	const nodes: GraphView['nodes'] = [{ id: 'me', name: 'Inès Roux', kind: 'person', category: 'identity', sensitive: false }];
	for (let i = 0; i < people; i++) nodes.push({ id: `p${i}`, name: `Personne ${i}`, kind: 'person', category: 'people', sensitive: false });
	for (let i = 0; i < places; i++) nodes.push({ id: `l${i}`, name: `Lieu ${i}`, kind: 'place', category: 'places', sensitive: i === 0 });
	for (let i = 0; i < events; i++)
		nodes.push({ id: `e${i}`, name: `Café ${i}`, kind: 'event', category: 'agenda', sensitive: false, start: '2026-11-20' });
	const edges: number[] = [];
	// Every event with the last person and the first place: they are the most linked.
	const last = people;
	for (let i = 0; i < events; i++) edges.push(last, 1 + people + places + i, 1 + people, 1 + people + places + i);
	return { nodes, edges, centered: true, categories: [] };
}

test('each kind in its own region around the owner, none overlapping', () => {
	const graph = memory(5000, 40, 600);
	const laid = layoutMemory(graph);
	expect([laid.x[0], laid.y[0]]).toEqual([0, 0]);
	expect(laid.regions.map((r) => r.kind)).toEqual(['person', 'place', 'event']);
	for (const [a, ra] of laid.regions.entries())
		for (const rb of laid.regions.slice(a + 1)) expect(Math.hypot(ra.x - rb.x, ra.y - rb.y)).toBeGreaterThan(ra.radius + rb.radius);
	for (const region of laid.regions) {
		expect(Math.hypot(region.x, region.y)).toBeGreaterThan(region.radius);
		for (const f of region.files) expect(Math.hypot(laid.x[f]! - region.x, laid.y[f]! - region.y)).toBeLessThanOrEqual(region.radius);
	}
	// The most linked first, at the heart of their region.
	expect(laid.regions[0]!.files[0]).toBe(5000);
	expect(laid.rank[5000]).toBe(0);
	expect(laid.regions[1]!.files[0]).toBe(5001);
	// The links of a file, both ways.
	expect(Array.from(laid.links.subarray(laid.start[5001]!, laid.start[5002]!)).length).toBe(600);
	// Always the same places.
	expect(Array.from(layoutMemory(graph).x)).toEqual(Array.from(laid.x));
});

test('the file under the pointer is found among those shown', () => {
	const graph = memory(300, 5, 20);
	const laid = layoutMemory(graph);
	const grid = new Grid(laid);
	const target = 42;
	const x = laid.x[target]! + 1;
	const y = laid.y[target]! - 1;
	expect(grid.nearest(laid, x, y, 6, () => true)).toBe(target);
	expect(grid.nearest(laid, x, y, 6, (i) => i !== target)).not.toBe(target);
	expect(grid.nearest(laid, 1e6, 1e6, 6, () => true)).toBe(-1);
});
