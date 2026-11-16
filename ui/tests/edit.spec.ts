import { expect, test } from '@playwright/test';
import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { startIpse, type Running } from './ipse.ts';

// Editing, creating and deleting files from the interface (issue #129), on an invented
// memory.
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
- Née à Lyon ^f-cd34

## Relations

- frère : [[paul-roux]]

## Notes

Rencontrée au club.
`;

let ipse: Running;
test.beforeEach(async () => {
	ipse = await startIpse('Inès Roux', 'fr', false, (vault) => writeFileSync(join(vault, 'people/marie-durand.md'), MARIE));
});
test.afterEach(() => ipse.stop());

const marie = () => readFileSync(join(ipse.vault, 'people/marie-durand.md'), 'utf8');

test('edit a person in place, then undo it from the history', async ({ page }) => {
	await page.goto(ipse.url);
	await page.goto(`${ipse.origin}/memory/people/01jcx8m2p4q6r8t0v2w4y6z801`);
	await page.getByRole('button', { name: 'Modifier' }).click();
	await expect(page.getByLabel('Nom', { exact: true })).toBeFocused();

	await page.getByRole('button', { name: '+ Ajouter un téléphone' }).click();
	await expect(page.getByLabel('Téléphones 1', { exact: true })).toBeFocused();
	await page.keyboard.type('+33 6 12 34 56 78');
	await page.getByLabel('Anniversaire').fill('--04-02');
	await page.getByLabel('Fait 1', { exact: true }).fill('Aime le jazz et la soul');
	await page.getByRole('button', { name: 'Retirer Née à Lyon' }).click();
	await page.getByRole('button', { name: '+ Ajouter un fait' }).click();
	await page.keyboard.type('Travaille à Nantes');
	await page.getByLabel('Notes').fill('Rencontrée au club.\n\nAppelle le dimanche.');
	await page.getByRole('button', { name: 'Enregistrer' }).click();

	await expect(page.getByText('Enregistré. Vous pouvez revenir en arrière depuis l’historique.')).toBeVisible();
	await expect(page.getByRole('button', { name: 'Modifier' })).toBeFocused();
	await expect(page.locator('.facts li')).toHaveText(['Aime le jazz et la soul', 'Travaille à Nantes']);
	await expect(page.getByText('Appelle le dimanche.')).toBeVisible();
	const after = marie();
	expect(after).toContain('+33 6 12 34 56 78');
	expect(after).toContain('- Aime le jazz et la soul ^f-ab12');
	expect(after).not.toContain('Née à Lyon');

	await page.getByRole('navigation', { name: 'Sections' }).getByRole('link', { name: 'Historique' }).click();
	const change = page.locator('li', { hasText: 'Fiche modifiée' }).first();
	await expect(change).toContainText('Vous');
	await change.getByRole('button', { name: 'Annuler ce changement' }).click();
	await page.getByRole('dialog').getByRole('button', { name: 'Annuler ce changement' }).click();
	await expect.poll(marie).toBe(MARIE);
});

test('a file changed since it was opened is not written over', async ({ page }) => {
	await page.goto(ipse.url);
	await page.goto(`${ipse.origin}/memory/people/01jcx8m2p4q6r8t0v2w4y6z801`);
	await page.getByRole('button', { name: 'Modifier' }).click();
	await page.getByLabel('Fait 1', { exact: true }).fill('Aime le rock');
	writeFileSync(join(ipse.vault, 'people/marie-durand.md'), MARIE.replace('Rencontrée', 'Vue'));
	await page.getByRole('button', { name: 'Enregistrer' }).click();
	await expect(page.getByText(/a changé depuis que vous l’avez ouverte/)).toBeVisible();
	expect(marie()).toContain('Vue au club');
	expect(marie()).toContain('Aime le jazz');
});

test('create the file a link points to, then delete it after confirming', async ({ page }) => {
	await page.goto(ipse.url);
	await page.goto(`${ipse.origin}/memory/people/01jcx8m2p4q6r8t0v2w4y6z801`);
	await page.getByRole('link', { name: 'Créer la fiche de Paul Roux' }).click();
	await expect(page.getByRole('heading', { level: 1 })).toHaveText('Ajouter une personne');
	await expect(page.getByLabel('Nom', { exact: true })).toHaveValue('Paul Roux');
	await page.getByRole('button', { name: 'Créer la fiche' }).click();
	await expect(page.getByRole('heading', { level: 1 })).toHaveText('Paul Roux');
	await expect(page.getByTestId('backlinks')).toContainText('Marie Durand');

	await page.getByRole('button', { name: 'Modifier' }).click();
	await page.getByRole('button', { name: 'Supprimer la fiche' }).click();
	const dialog = page.getByRole('dialog');
	await expect(dialog).toContainText('Marie Durand renvoie à cette fiche');
	await expect(dialog.getByRole('button', { name: 'Garder' })).toBeFocused();
	await dialog.getByRole('button', { name: 'Supprimer' }).click();
	await expect(page).toHaveURL(`${ipse.origin}/memory/people`);
	await expect(page.getByTestId('people')).not.toContainText('Paul Roux');
});

test('add a place from its list, and an event with its people from the agenda', async ({ page }) => {
	await page.goto(ipse.url);
	await page.goto(`${ipse.origin}/memory/places`);
	await page.getByRole('link', { name: '+ Ajouter un lieu' }).click();
	await page.getByLabel('Nom', { exact: true }).fill('Salle Pleyel');
	await page.getByLabel('Adresse').fill('252 rue du Faubourg Saint-Honoré, Paris');
	await page.getByRole('button', { name: 'Créer la fiche' }).click();
	await expect(page.getByRole('heading', { level: 1 })).toHaveText('Salle Pleyel');

	await page.getByRole('navigation', { name: 'Sections' }).getByRole('link', { name: 'Agenda' }).click();
	await page.getByRole('link', { name: '+ Ajouter un événement' }).click();
	await page.getByLabel('Titre').fill('Concert de jazz');
	await page.getByLabel('Début', { exact: true }).fill('2026-12-04');
	await page.getByLabel('Heure de début').fill('20:00');
	await page.getByRole('combobox', { name: 'Lieu' }).fill('Pleyel');
	await page.getByRole('option', { name: /Salle Pleyel/ }).click();
	await page.getByRole('combobox', { name: 'Ajouter quelqu’un' }).fill('Marie');
	await page.getByRole('option', { name: /Marie Durand/ }).click();
	await page.getByRole('combobox', { name: 'Ajouter quelqu’un' }).fill('Hugo');
	await page.keyboard.press('ArrowDown');
	await expect(page.getByRole('option', { name: 'Garder « Hugo » comme simple nom' })).toBeVisible();
	await page.keyboard.press('Enter');
	await page.getByRole('button', { name: 'Créer la fiche' }).click();

	await expect(page.getByRole('heading', { level: 1 })).toHaveText('Concert de jazz');
	await expect(page.getByText(/20:00|20 h/)).toBeVisible();
	await expect(page.getByRole('link', { name: 'Salle Pleyel' })).toBeVisible();
	await expect(page.getByRole('link', { name: /Marie Durand/ })).toBeVisible();
	await expect(page.getByText('Hugo', { exact: true })).toBeVisible();

	await page.getByRole('button', { name: 'Modifier' }).click();
	await page.getByRole('button', { name: 'Retirer Hugo' }).click();
	await page.getByRole('button', { name: 'Annulé' }).click();
	await page.getByRole('button', { name: 'Enregistrer' }).click();
	await expect(page.getByText('Annulé', { exact: true })).toBeVisible();
	await expect(page.getByText('Hugo', { exact: true })).toHaveCount(0);
});
