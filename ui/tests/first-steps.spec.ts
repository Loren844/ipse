import { expect, test } from '@playwright/test';
import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { IPSE, startDemo, startIpse, type Running } from './ipse.ts';

// The first steps on the home screen and the note the first time a screen opens
// (issue #172).
let ipse: Running;
test.afterEach(() => ipse.stop());

test('the first steps follow what the memory shows, then go away when put away', async ({ page }) => {
	ipse = await startIpse('Inès Roux', 'fr');
	await page.goto(ipse.url);
	const steps = page.getByTestId('first-steps');
	await expect(steps.getByRole('heading', { level: 2 })).toHaveText('Premiers pas');
	await expect(steps).toContainText('0 sur 4');
	await expect(steps.locator('li.now')).toHaveAttribute('data-step', 'sources');
	await expect(steps.getByRole('link', { name: 'Ouvrir les sources' })).toHaveAttribute('href', '/sources');
	await expect(steps.getByRole('button', { name: /Copier/ })).toBeVisible();

	// An AI connected from the command line: the step is done, the others wait.
	execFileSync(IPSE, ['client', 'add', 'Claude', '--vault', ipse.vault]);
	await page.reload();
	await expect(steps).toContainText('1 sur 4');
	await expect(steps.locator('li[data-step="ai"]')).toHaveClass(/done/);
	await expect(steps.locator('li[data-step="ai"]')).toContainText('(fait)');
	await expect(steps.locator('li.now')).toHaveAttribute('data-step', 'sources');

	await steps.getByRole('button', { name: 'Masquer' }).click();
	await expect(steps).toHaveCount(0);
	await page.reload();
	await expect(page.getByRole('heading', { level: 1 })).toBeVisible();
	await expect(steps).toHaveCount(0);
	expect(JSON.parse(readFileSync(join(ipse.vault, '.ipse/guide.json'), 'utf8')).hidden).toBe(true);
});

test('a screen explains itself the first time, until the person read it', async ({ page }) => {
	ipse = await startIpse('Inès Roux', 'fr');
	await page.goto(ipse.url);
	await page.getByRole('navigation').getByRole('link', { name: /À revoir/ }).click();
	const note = page.getByTestId('first-visit');
	await expect(note).toContainText('Rien n’est écrit tant que vous n’avez pas accepté');
	await note.getByRole('button', { name: 'Compris' }).click();
	await expect(note).toHaveCount(0);
	await page.reload();
	await expect(page.getByRole('heading', { level: 1 })).toBeVisible();
	await expect(note).toHaveCount(0);

	// Another screen still has its own.
	await page.getByRole('navigation').getByRole('link', { name: 'Agenda' }).click();
	await expect(note).toContainText('Votre agenda, en liste');
	expect(JSON.parse(readFileSync(join(ipse.vault, '.ipse/guide.json'), 'utf8')).seen).toEqual(['review']);
});

test('an invented memory says so, and how to make one’s own', async ({ page }) => {
	ipse = await startDemo('en');
	await page.goto(ipse.url);
	const steps = page.getByTestId('first-steps');
	await expect(steps.getByRole('heading', { level: 2 })).toHaveText('An invented memory');
	await expect(steps).toContainText('With the ipse command');
	await expect(steps.locator('pre')).toHaveText('ipse init ~/Ipse');
	await expect(steps.locator('li')).toHaveCount(0);
});

test('how it works, from the menu and from the first steps', async ({ page }) => {
	ipse = await startIpse('Inès Roux', 'fr');
	await page.goto(ipse.url);
	await page.getByTestId('first-steps').getByRole('link', { name: 'Comment ça marche' }).click();
	await expect(page.getByRole('heading', { level: 1 })).toHaveText('Comment ça marche');
	await expect(page.getByRole('heading', { level: 2 })).toHaveText([
		'Votre mémoire est un dossier à vous',
		'Vos sources la remplissent',
		'Chaque IA lit ce que vous lui ouvrez',
		'Elle propose, vous décidez'
	]);
	await expect(page.locator('.how-folder code')).toHaveText(ipse.vault);
	await page.getByRole('link', { name: 'Gérer les IA connectées' }).click();
	await expect(page.getByRole('heading', { level: 1 })).toHaveText('IA connectées');

	await page.getByRole('button', { name: /^Mémoire de Inès Roux/ }).click();
	await page.getByRole('menuitem', { name: 'Comment ça marche' }).click();
	await expect(page.getByRole('heading', { level: 1 })).toHaveText('Comment ça marche');
});
