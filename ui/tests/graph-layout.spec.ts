// Where the graph centred on one file puts its neighbours (issue #213), without a
// browser: one row each, so names never overlap, the strongest nearer. Data invented.
import { expect, test } from '@playwright/test';
import type { NeighbourView } from '../src/lib/api.ts';
import { heightFor, layoutAround, ROW } from '../src/lib/egoGraph.ts';

function neighbours(count: number): NeighbourView[] {
	return Array.from({ length: count }, (_, i) => ({
		file: { id: `p${i}`, name: `Personne ${i}`, kind: i % 3 ? 'person' : 'place', category: 'people', sensitive: false },
		words: [],
		back: [],
		together: count - i
	}));
}

test('one row each, on both sides, the strongest nearest', () => {
	const shown = neighbours(30);
	const height = heightFor(shown.length, 300);
	expect(height).toBeGreaterThanOrEqual(15 * ROW);
	const places = layoutAround(shown, 900, height);
	expect(places.size).toBe(30);
	const all = [...places.values()];
	for (const side of [1, -1]) {
		const ys = all.filter((p) => Math.sign(p.x) === side).map((p) => p.y).sort((a, b) => a - b);
		expect(ys.length).toBe(15);
		for (let i = 1; i < ys.length; i++) expect(ys[i]! - ys[i - 1]!).toBeGreaterThanOrEqual(ROW - 0.5);
	}
	// Two people on the same side, at the same height: the stronger is nearer.
	const strong = places.get('p1')!;
	const weak = places.get('p29')!;
	expect(Math.abs(strong.x) / Math.sqrt(1 - 0.85 * (strong.y / (height / 2 - 24)) ** 2)).toBeLessThan(
		Math.abs(weak.x) / Math.sqrt(1 - 0.85 * (weak.y / (height / 2 - 24)) ** 2)
	);
	// Always the same places.
	expect([...layoutAround(shown, 900, height).values()]).toEqual(all);
});
