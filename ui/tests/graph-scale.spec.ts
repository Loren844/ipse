// The graph of a large memory (issue #174): 20 000 people and 2 000 events, moved and
// zoomed in a real browser; each frame's drawing time is measured by the page itself.
// Data invented.
import { expect, test } from '@playwright/test';
import { mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { startIpse, type Running } from './ipse.ts';

const PEOPLE = 20_000;
const EVENTS = 2_000;
const FIRST = ['Marie', 'Paul', 'Zoé', 'Hugo', 'Léa', 'Nina', 'Tom', 'Inès'];
const LAST = ['Durand', 'Martin', 'Petit', 'Roux', 'Faure', 'Blanc', 'Garnier', 'Morel'];
const id = (prefix: string, i: number) => `01jcx8m2p4q6r8t0v2w${prefix}${String(i).padStart(6, '0')}`;

function fill(vault: string) {
	const stamp = 'created: 2026-01-10T09:00:00+01:00\nupdated: 2026-01-10T09:00:00+01:00';
	for (let i = 0; i < PEOPLE; i++) {
		const name = `${FIRST[i % 8]} ${LAST[Math.floor(i / 8) % 8]} ${i}`;
		writeFileSync(
			join(vault, 'people', `p-${i}.md`),
			`---\nid: ${id('4', i)}\ntype: person\ncategory: people\n${stamp}\nname: ${name}\n---\n\n## Relations\n\n- ami : [[p-${(i + 1) % PEOPLE}]]\n`
		);
	}
	mkdirSync(join(vault, 'events', '2026'), { recursive: true });
	for (let e = 0; e < EVENTS; e++) {
		const day = `2026-${String((e % 12) + 1).padStart(2, '0')}-${String((e % 28) + 1).padStart(2, '0')}`;
		writeFileSync(
			join(vault, 'events', '2026', `e-${e}.md`),
			`---\nid: ${id('5', e)}\ntype: event\ncategory: agenda\n${stamp}\ntitle: Café ${e}\nstart: ${day}\nparticipants:\n  - "[[p-0]]"\n  - "[[p-${e * 10 + 3}]]"\n---\n`
		);
	}
}

let ipse: Running;
test.beforeEach(async () => {
	test.setTimeout(240_000);
	ipse = await startIpse('Jules Moreau', 'fr', false, fill);
});
test.afterEach(() => ipse.stop());

test('22 000 files are drawn, moved and zoomed smoothly', async ({ page }) => {
	await page.setViewportSize({ width: 1440, height: 900 });
	await page.goto(ipse.url);
	const asked = Date.now();
	await page.goto(`${ipse.origin}/memory?graph=1`);
	const canvas = page.getByTestId('memory-graph');
	await expect(canvas).toHaveAttribute('data-files', String(PEOPLE + EVENTS + 1), { timeout: 120_000 });
	const shown = Date.now() - asked;

	const box = (await canvas.boundingBox())!;
	const x = box.x + box.width / 2;
	const y = box.y + box.height / 2;
	await page.mouse.move(x, y);
	// Zoom in, move around, zoom out: every frame measured.
	for (let i = 0; i < 12; i++) await page.mouse.wheel(0, -180);
	await page.mouse.down();
	for (let i = 0; i < 20; i++) await page.mouse.move(x + i * 15, y + i * 8);
	await page.mouse.up();
	for (let i = 0; i < 12; i++) await page.mouse.wheel(0, 240);
	await page.waitForTimeout(300);

	const frames = Number(await canvas.getAttribute('data-frames'));
	const median = Number(await canvas.getAttribute('data-frame'));
	const worst = Number(await canvas.getAttribute('data-worst'));
	console.log(`graph of ${PEOPLE + EVENTS + 1} files: shown in ${shown} ms; ${frames} frames, median ${median} ms, worst ${worst} ms`);
	expect(frames).toBeGreaterThan(20);
	// A frame of a 60 Hz screen lasts 16.7 ms; a test machine is slower than a person's.
	expect(median).toBeLessThan(33);
});
