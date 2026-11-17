// The map (issue #135): places from location history, sized by visits; a place opens
// its visits and the people there; tiles only once accepted. Data invented.
import { expect, test } from '@playwright/test';
import { execFileSync } from 'node:child_process';
import { mkdirSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { IPSE, startIpse, type Running } from './ipse.ts';

const LOCATION = resolve(import.meta.dirname, '../../crates/ipse/tests/fixtures/location');

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
		writeFileSync(join(vault, 'places/sans-position.md'), file('903', 'type: place\ncategory: places\nname: Sans position'));
		for (const export_ of ['Timeline.json', 'location-history.json'])
			execFileSync(IPSE, ['import', join(LOCATION, export_), '--vault', vault]);
	});
});
test.afterEach(() => ipse.stop());

test('the places on a map, a place with its visits and people, no tiles until accepted', async ({ page }) => {
	const tiles: string[] = [];
	page.on('request', (request) => {
		if (request.url().includes('tile.openstreetmap.org')) tiles.push(request.url());
	});
	await page.route('https://tile.openstreetmap.org/**', (route) => route.fulfill({ status: 204 }));
	await page.goto(ipse.url);
	await page.getByRole('link', { name: 'Carte' }).first().click();
	await expect(page.getByRole('heading', { level: 1 })).toHaveText('Carte');
	// The places came from location history: closed to AIs, and said so.
	await expect(page.getByRole('note').first()).toContainText('« position » leur est fermée');
	const map = page.getByRole('application', { name: /Carte de vos lieux/ });
	await expect(map.locator('circle')).not.toHaveCount(0);
	await expect(page.getByText('1 lieu n’a pas de position : il n’est pas sur la carte.')).toBeVisible();

	// Most visited first; a place opens its visits and who was there.
	const list = page.getByRole('list').filter({ has: page.getByRole('button', { name: '12 rue Inventée' }) });
	await list.getByRole('button', { name: '12 rue Inventée' }).click();
	const side = page.locator('.map-side');
	await expect(side.getByRole('heading', { level: 2 })).toHaveText('12 rue Inventée');
	await expect(side).toContainText('1 passage');
	await expect(side.getByRole('link', { name: 'Julie Moreau' })).toBeVisible();
	await expect(side.getByRole('link', { name: 'Dîner chez Julie' })).toBeVisible();

	// A circle of the map chooses its place too.
	await map.locator('circle').first().click({ force: true });
	await expect(side.getByRole('heading', { level: 2 })).not.toBeEmpty();

	// Zoom from the keyboard and the buttons.
	await page.getByRole('button', { name: 'Zoomer', exact: true }).click();
	await map.focus();
	await page.keyboard.press('-');
	await page.keyboard.press('ArrowLeft');

	// No tile was asked for until the person accepts; then they come from OpenStreetMap.
	expect(tiles).toEqual([]);
	await page.getByRole('button', { name: 'Afficher le fond de carte' }).click();
	await expect(map.locator('image').first()).toBeAttached();
	await expect.poll(() => tiles.length).toBeGreaterThan(0);
	await expect(page.getByRole('link', { name: 'les contributeurs d’OpenStreetMap' })).toBeVisible();
	// The choice stays in this browser; it can be undone.
	await page.reload();
	await expect(page.getByRole('button', { name: 'Masquer le fond de carte' })).toBeVisible();
	await page.getByRole('button', { name: 'Masquer le fond de carte' }).click();
	await expect(map.locator('image')).toHaveCount(0);
});

test('without any position, the map says how to fill it', async ({ page }) => {
	const empty = await startIpse('Inès Roux', 'fr');
	await page.goto(empty.url);
	await page.goto(`${empty.origin}/map`);
	await expect(page.getByText('Aucun lieu n’a encore de position.')).toBeVisible();
	empty.stop();
});
