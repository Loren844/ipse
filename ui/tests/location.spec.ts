// Location history in the interface (issue #106): a dinner of the agenda gets its place
// from the phone's history, and the person's page says where you meet.
import { expect, test } from '@playwright/test';
import { execFileSync } from 'node:child_process';
import { mkdirSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { IPSE, startIpse, type Running } from './ipse.ts';

const TIMELINE = resolve(import.meta.dirname, '../../crates/ipse/tests/fixtures/location/Timeline.json');

const file = (id: string, fields: string) =>
	`---\nid: 01jcx8m2p4q6r8t0v2w4y6z${id}\n${fields}\ncreated: 2026-01-10T09:00:00+01:00\nupdated: 2026-01-10T09:00:00+01:00\n---\n`;

let ipse: Running;
test.beforeEach(async () => {
	ipse = await startIpse('Inès Roux', 'fr', false, (vault) => {
		mkdirSync(join(vault, 'events/2026'), { recursive: true });
		writeFileSync(join(vault, 'people/julie-moreau.md'), file('901', 'type: person\ncategory: people\nname: Julie Moreau'));
		writeFileSync(
			join(vault, 'events/2026/2026-03-06-diner-chez-julie.md'),
			file(
				'902',
				'type: event\ncategory: agenda\ntitle: Dîner chez Julie\nstart: 2026-03-06T20:00:00+01:00\nend: 2026-03-06T23:00:00+01:00\nplace: 12 rue Inventée\nparticipants:\n  - "[[julie-moreau]]"'
			)
		);
		execFileSync(IPSE, ['import', TIMELINE, '--vault', vault]);
	});
});
test.afterEach(() => ipse.stop());

test("a person's page shows where you meet, from the location history", async ({ page }) => {
	await page.goto(ipse.url);
	await page.goto(`${ipse.origin}/memory/people/01jcx8m2p4q6r8t0v2w4y6z901`);
	const places = page.getByTestId('places');
	await expect(page.getByRole('heading', { name: 'Où vous vous voyez' })).toBeVisible();
	// The dinner's address named the place the phone saw.
	await expect(places.getByRole('link', { name: '12 rue Inventée' })).toBeVisible();
	await places.getByRole('link', { name: '12 rue Inventée' }).click();
	await expect(page.getByRole('heading', { level: 1, name: '12 rue Inventée' })).toBeVisible();
});
