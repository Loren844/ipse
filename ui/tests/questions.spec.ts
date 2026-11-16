import { expect, test } from '@playwright/test';
import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { startIpse, type Running } from './ipse.ts';

// Two invented people saved under a nickname and a first name only.
let ipse: Running;
test.beforeEach(async () => {
	ipse = await startIpse('Inès Roux', 'fr', false, (vault) => {
		writeFileSync(
			join(vault, 'people/jo.md'),
			'---\nid: 01jcx8m2p4q6r8t0v2w4y6z801\ntype: person\ncategory: people\ncreated: 2030-01-01T09:00:00+01:00\nupdated: 2030-01-01T09:00:00+01:00\nname: Jo plombier\nphones:\n  - "+33600000001"\n---\n'
		);
		writeFileSync(
			join(vault, 'people/lou.md'),
			'---\nid: 01jcx8m2p4q6r8t0v2w4y6z802\ntype: person\ncategory: people\ncreated: 2030-01-01T09:00:00+01:00\nupdated: 2030-01-01T09:00:00+01:00\nname: Lou\n---\n'
		);
	});
});
test.afterEach(() => ipse.stop());

const read = (file: string) => readFileSync(join(ipse.vault, file), 'utf8');

test('name someone saved under a nickname, or say you do not know', async ({ page }) => {
	await page.goto(ipse.url);
	const inbox = page.getByRole('navigation', { name: 'Sections' }).getByRole('link', { name: /À revoir/ });
	await expect(inbox).toContainText('2');
	await inbox.click();
	await expect(page.getByRole('heading', { level: 2, name: 'Qui est-ce ?' })).toBeVisible();

	const jo = page.getByTestId('question').filter({ hasText: 'Jo plombier' });
	await expect(jo.getByRole('heading')).toHaveText('Qui est «\u00a0Jo plombier\u00a0»\u00a0?');
	await expect(jo).toContainText('+33 6 00 00 00 01');
	const save = jo.getByRole('button', { name: 'Enregistrer' });
	await expect(save).toBeDisabled();
	await jo.getByLabel('Son nom complet').fill('Jo Lambert');
	await jo.getByLabel('Qui est-ce pour vous ?').fill('plombier');
	await save.click();
	await expect(page.getByRole('status').first()).toContainText('la fiche s’appelle maintenant Jo Lambert');
	await expect(page.getByRole('status').first()).toBeFocused();
	await expect(page.getByTestId('question')).toHaveCount(1);
	expect(read('people/jo.md')).toContain('name: Jo Lambert\n');
	expect(read('people/jo.md')).toContain('  - Jo plombier\n');
	expect(read('people/ines-roux.md')).toContain('- plombier : [[jo]]');
	await expect(inbox).toContainText('1');

	const lou = page.getByTestId('question').filter({ hasText: 'Lou' });
	await lou.getByRole('button', { name: 'Je ne sais pas' }).click();
	await expect(page.getByRole('status').first()).toContainText('on ne vous reposera pas la question');
	await expect(page.getByTestId('question')).toHaveCount(0);
	expect(read('people/lou.md')).toContain('name: Lou\n');

	// Asked once: not again after a reload.
	await page.reload();
	await expect(page.getByText('Rien à revoir pour le moment.')).toBeVisible();
});
