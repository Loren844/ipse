import { expect, test } from '@playwright/test';
import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { startIpse, type Running } from './ipse.ts';

// Modifier opens the form or the file as written, in Markdown (issue #142), on an
// invented memory.
const MARIE = `---
id: 01jcx8m2p4q6r8t0v2w4y6z801
type: person
category: people
name: Marie Durand
created: 2026-01-10T09:00:00+01:00
updated: 2026-01-10T09:00:00+01:00
---

## Faits

- Aime le jazz ^f-ab12

## Notes

Rencontrée au club.
`;

let ipse: Running;
test.beforeEach(async () => {
	ipse = await startIpse('Inès Roux', 'fr', false, (vault) => writeFileSync(join(vault, 'people/marie-durand.md'), MARIE));
});
test.afterEach(() => ipse.stop());

const marie = () => readFileSync(join(ipse.vault, 'people/marie-durand.md'), 'utf8');

test('edit a file as Markdown, from the same Modifier, on the tab chosen last', async ({ page }) => {
	await page.goto(ipse.url);
	await page.goto(`${ipse.origin}/memory/people/01jcx8m2p4q6r8t0v2w4y6z801`);
	// One button on the file's page.
	await expect(page.getByRole('button', { name: /Markdown/ })).toHaveCount(0);
	await page.getByRole('button', { name: 'Modifier' }).click();
	const tabs = page.getByRole('tablist', { name: 'Modifier comme' });
	await expect(tabs.getByRole('tab')).toHaveText(['Fiche', 'Markdown']);
	await expect(tabs.getByRole('tab', { name: 'Fiche' })).toHaveAttribute('aria-selected', 'true');
	await expect(page.getByLabel('Nom', { exact: true })).toBeFocused();

	// A change not saved yet: switching asks first, and staying keeps it.
	await page.getByLabel('Nom', { exact: true }).fill('Marie Durand-Roux');
	await tabs.getByRole('tab', { name: 'Markdown' }).click();
	const dialog = page.getByRole('dialog', { name: 'Changer d’onglet ?' });
	await dialog.getByRole('button', { name: 'Rester' }).click();
	await expect(page.getByLabel('Nom', { exact: true })).toHaveValue('Marie Durand-Roux');
	await tabs.getByRole('tab', { name: 'Fiche' }).press('ArrowRight');
	await dialog.getByRole('button', { name: 'Abandonner et changer' }).click();
	await expect(tabs.getByRole('tab', { name: 'Markdown' })).toHaveAttribute('aria-selected', 'true');

	// The file as written, front matter included.
	const text = page.getByLabel('La fiche telle qu’elle est écrite');
	await expect(text).toHaveValue(MARIE);

	// A changed id is refused in words; what was typed stays.
	const typed = MARIE.replace('## Notes', '## Voyages\n\nLisbonne en 2024.\n\n## Notes').replace('- Aime le jazz ^f-ab12', '- Aime le jazz ^f-ab12\n- Court le dimanche');
	await text.fill(typed.replace('z801', 'z802'));
	await page.getByRole('button', { name: 'Enregistrer' }).click();
	await expect(page.getByRole('status').filter({ hasText: '« id » ne peut pas changer ici' })).toBeVisible();
	await expect(text).toHaveValue(typed.replace('z801', 'z802'));
	await expect(text).toBeFocused();
	expect(marie()).toBe(MARIE);

	// Text that does not read as a file: the line is said.
	await text.fill(typed.replace('name: Marie Durand', 'name: [Marie'));
	await page.getByRole('button', { name: 'Enregistrer' }).click();
	await expect(page.getByRole('status').filter({ hasText: /Ligne \d+ : Ipse ne peut pas lire ce texte comme une fiche/ })).toBeVisible();

	await text.fill(typed);
	await page.getByRole('button', { name: 'Enregistrer' }).click();
	await expect(page.getByText('Enregistré. Vous pouvez revenir en arrière depuis l’historique.')).toBeVisible();
	await expect(page.locator('.facts li')).toHaveText(['Aime le jazz', 'Court le dimanche']);
	expect(marie()).toContain('## Voyages\n\nLisbonne en 2024.\n');
	expect(marie()).toMatch(/- Court le dimanche \^f-\w+/);

	// The tab chosen last opens next time.
	await page.getByRole('button', { name: 'Modifier' }).click();
	await expect(tabs.getByRole('tab', { name: 'Markdown' })).toHaveAttribute('aria-selected', 'true');
	await expect(text).toBeFocused();
	await expect(text).toHaveValue(/Lisbonne en 2024/);
	await page.getByRole('button', { name: 'Annuler' }).click();

	// One change of the history, by the person, that can be undone.
	await page.getByRole('navigation', { name: 'Sections' }).getByRole('link', { name: 'Historique' }).click();
	const change = page.locator('li', { hasText: 'Fiche modifiée en Markdown' }).first();
	await expect(change).toContainText('Vous');
	await change.getByRole('button', { name: 'Annuler ce changement' }).click();
	await page.getByRole('dialog').getByRole('button', { name: 'Annuler ce changement' }).click();
	await expect.poll(marie).toBe(MARIE);
});

test('a file changed meanwhile is not written over from the Markdown tab', async ({ page }) => {
	await page.goto(ipse.url);
	await page.goto(`${ipse.origin}/memory/people/01jcx8m2p4q6r8t0v2w4y6z801`);
	await page.getByRole('button', { name: 'Modifier' }).click();
	await page.getByRole('tab', { name: 'Markdown' }).click();
	const text = page.getByLabel('La fiche telle qu’elle est écrite');
	await expect(text).toHaveValue(MARIE);
	writeFileSync(join(ipse.vault, 'people/marie-durand.md'), MARIE.replace('Rencontrée au club.', 'Rencontrée à Lyon.'));
	await text.fill(MARIE.replace('Rencontrée au club.', 'Vue au concert.'));
	await page.getByRole('button', { name: 'Enregistrer' }).click();
	await expect(page.getByRole('status').filter({ hasText: 'Cette fiche a changé depuis que vous l’avez ouverte' })).toBeVisible();
	expect(marie()).toContain('Rencontrée à Lyon.');
});
