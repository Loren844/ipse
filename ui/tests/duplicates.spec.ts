import { expect, test } from '@playwright/test';
import { execFileSync } from 'node:child_process';
import { existsSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { IPSE, startIpse, type Running } from './ipse.ts';

// The test contacts hold Marc Leroy (marc.leroy@example.org). A second, invented file
// with his email, and a third that links to it: a possible duplicate, found on demand.
let ipse: Running;
test.beforeEach(async () => {
	ipse = await startIpse('Inès Roux', 'fr', true, (vault) => {
		writeFileSync(
			join(vault, 'people/marc.md'),
			'---\nid: 01jcx8m2p4q6r8t0v2w4y6z899\ntype: person\ncategory: people\ncreated: 2030-01-01T09:00:00+01:00\nupdated: 2030-01-01T09:00:00+01:00\nname: Marc\nemails:\n  - Marc.Leroy@example.org\nphones:\n  - "+33 7 00 00 00 01"\n---\n\n## Faits\n\n- Joue au tennis le jeudi.\n'
		);
		writeFileSync(
			join(vault, 'people/sophie-bernard.md'),
			'---\nid: 01jcx8m2p4q6r8t0v2w4y6z898\ntype: person\ncategory: people\ncreated: 2030-01-01T09:00:00+01:00\nupdated: 2030-01-01T09:00:00+01:00\nname: Sophie Bernard\n---\n\n## Relations\n\n- collègue : [[marc]]\n'
		);
		execFileSync(IPSE, ['duplicates', '--vault', vault]);
	});
});
test.afterEach(() => ipse.stop());

const read = (file: string) => readFileSync(join(ipse.vault, file), 'utf8');

async function open(page: import('@playwright/test').Page) {
	await page.goto(ipse.url);
	await page.getByRole('navigation', { name: 'Sections' }).getByRole('link', { name: /À revoir/ }).click();
	const line = page.getByTestId('proposal').filter({ hasText: 'Même adresse email' });
	await expect(line).toContainText('Proposée par Ipse');
	await expect(line).toContainText('Marc Leroy');
	await expect(line).toContainText('Marc');
	await line.getByRole('link').click();
	await expect(page.getByRole('heading', { level: 1 })).toHaveText('Même adresse email : très probablement la même personne.');
}

test('merge two files of the same person', async ({ page }) => {
	await open(page);
	await expect(page.getByText('« Marc » sera fusionnée dans « Marc Leroy », puis retirée.')).toBeVisible();
	await expect(page.getByText('1 autre fiche qui pointait vers elle pointera vers celle-ci.')).toBeVisible();
	await expect(page.locator('.diff .added')).toContainText([/Joue au tennis le jeudi\./]);
	await page.getByRole('button', { name: 'Fusionner' }).click();
	await expect(page.getByRole('status')).toContainText('Fusionnées : une seule fiche reste');
	expect(existsSync(join(ipse.vault, 'people/marc.md'))).toBe(false);
	expect(read('people/marc-leroy.md')).toContain('- Joue au tennis le jeudi.');
	expect(read('people/marc-leroy.md')).toContain('  - Marc\n');
	expect(read('people/sophie-bernard.md')).toContain('- collègue : [[marc-leroy]]');

	// Undone: both files and the link come back.
	await page.getByRole('button', { name: 'Annuler' }).click();
	await expect(page.getByRole('status')).toContainText('Proposition annulée');
	expect(existsSync(join(ipse.vault, 'people/marc.md'))).toBe(true);
	expect(read('people/sophie-bernard.md')).toContain('- collègue : [[marc]]');
});

test('these are two people: never proposed again', async ({ page }) => {
	await open(page);
	await page.getByRole('button', { name: 'Ce sont deux personnes' }).click();
	await expect(page.getByRole('status')).toContainText('C’est noté : Ipse ne vous les proposera plus.');
	expect(read('.ipse/not-duplicates.toml')).toContain('01jcx8m2p4q6r8t0v2w4y6z899');
	expect(existsSync(join(ipse.vault, 'people/marc.md'))).toBe(true);
	const again = execFileSync(IPSE, ['duplicates', '--vault', ipse.vault]).toString();
	expect(again.trim()).toBe('No new possible duplicate.');
});
