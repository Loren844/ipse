// The graph of a large memory (issue #213): 20 000 people and 2 000 events. Centred on
// one file, it draws its strongest links at once, whatever the size of the memory.
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

test('a graph of 22 000 files opens and recentres at once', async ({ page }) => {
	await page.setViewportSize({ width: 1440, height: 900 });
	await page.goto(ipse.url);
	let asked = Date.now();
	await page.goto(`${ipse.origin}/graph`);
	const drawing = page.getByTestId('graph');
	await expect(drawing).toHaveAttribute('data-files', '31', { timeout: 60_000 });
	const opened = Date.now() - asked;

	// The busiest person: in every event, a friend on each side.
	const busiest = page.getByTestId('graph-links').getByRole('button', { name: /Marie Durand 0$/ });
	asked = Date.now();
	await busiest.click();
	await expect(page.getByTestId('graph-panel').getByRole('heading', { level: 2 })).toHaveText('Marie Durand 0');
	await expect(drawing).toHaveAttribute('data-moving', 'false');
	const recentred = Date.now() - asked;
	// Hundreds of links (the events of the last twelve months), 30 drawn.
	await expect(page.getByTestId('graph-panel').getByRole('heading', { level: 3 }).first()).toHaveText(/^\d\s?\d{3} liens$/);
	await expect(drawing).toHaveAttribute('data-files', '31');
	console.log(`graph of ${PEOPLE + EVENTS + 1} files: opened in ${opened} ms, recentred in ${recentred} ms`);
	// A test machine is slower than a person's: still well under a second each.
	expect(recentred).toBeLessThan(3000);
});
