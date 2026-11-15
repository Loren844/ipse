import { expect, test } from '@playwright/test';
import { execFileSync } from 'node:child_process';
import { appendFileSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { IPSE, startIpse, type Running } from './ipse.ts';

let ipse: Running;
test.beforeEach(async () => {
	ipse = await startIpse('Inès Roux', 'fr', false, (vault) => {
		execFileSync(IPSE, ['client', 'add', 'Claude Desktop', '--vault', vault]);
		// A category of the person's own, declared without saying whether it is sensitive.
		appendFileSync(join(vault, '.ipse/vault.toml'), '\n[categories.club]\nname = "Club"\n');
	});
});
test.afterEach(() => ipse.stop());

const clients = () => readFileSync(join(ipse.vault, '.ipse/clients.toml'), 'utf8');
const settings = () => readFileSync(join(ipse.vault, '.ipse/vault.toml'), 'utf8');
const readList = () => /read = \[(.*)\]/.exec(clients())![1];

test('open a sensitive category to one AI, close it, and let it propose', async ({ page }) => {
	await page.goto(ipse.url);
	await page.getByRole('navigation').getByRole('link', { name: 'IA connectées' }).click();
	await expect(page.getByRole('heading', { level: 1 })).toHaveText('IA connectées');
	const claude = page.getByTestId('ai');
	await expect(claude.getByRole('heading', { level: 2 })).toHaveText('Claude Desktop');
	await expect(claude).toContainText('Pas encore utilisée');
	await expect(claude.getByRole('switch', { name: 'Claude Desktop peut lire : Personnes' })).toBeChecked();
	await expect(claude.getByRole('row', { name: /Club/ })).toContainText('Sensible, faute de précision');

	// A sensitive category asks first; Escape or "Garder fermée" keeps it closed.
	const health = claude.getByRole('switch', { name: 'Claude Desktop peut lire : Santé' });
	await expect(health).not.toBeChecked();
	await health.click();
	const dialog = page.getByRole('dialog', { name: 'Ouvrir « Santé » à Claude Desktop ?' });
	await expect(dialog).toBeVisible();
	await expect(dialog.getByRole('button', { name: 'Garder fermée' })).toBeFocused();
	await page.keyboard.press('Escape');
	await expect(dialog).toBeHidden();
	await expect(health).not.toBeChecked();
	expect(readList()).not.toContain('health');

	await health.click();
	await dialog.getByRole('button', { name: 'Ouvrir à Claude Desktop' }).click();
	await expect(page.getByRole('status')).toContainText('Enregistré : s’applique à Claude Desktop dès sa prochaine requête.');
	await expect(health).toBeChecked();
	expect(readList()).toContain('"health"');
	await expect(page.getByTestId('categories').getByRole('listitem').filter({ hasText: 'Santé' })).toContainText('Lue par Claude Desktop');

	// Closing needs no question.
	await health.click();
	await expect(health).not.toBeChecked();
	await expect(dialog).toBeHidden();
	expect(readList()).not.toContain('health');
	await expect(page.getByTestId('categories').getByRole('listitem').filter({ hasText: 'Santé' })).toContainText('Fermée à toutes vos IA');

	// Proposing, from the keyboard.
	await claude.getByRole('switch', { name: 'Claude Desktop peut proposer des changements : Personnes' }).focus();
	await page.keyboard.press('Space');
	await expect(claude.getByRole('switch', { name: 'Claude Desktop peut proposer des changements : Personnes' })).toBeChecked();
	expect(clients()).toContain('propose = ["people"]');
	// Reading no more stops proposing too.
	await claude.getByRole('switch', { name: 'Claude Desktop peut lire : Personnes' }).click();
	await expect(claude.getByRole('switch', { name: 'Claude Desktop peut proposer des changements : Personnes' })).not.toBeChecked();
	expect(clients()).toContain('propose = []');
});

test('add a category of your own, open it to the AIs you choose', async ({ page }) => {
	await page.goto(ipse.url);
	await page.getByRole('navigation').getByRole('link', { name: 'IA connectées' }).click();
	await page.getByLabel('Son nom', { exact: true }).last().fill('Études');
	await page.getByLabel('Oui : comme vos contacts ou votre agenda').check();
	await page.getByRole('checkbox', { name: 'Claude Desktop', exact: true }).check();
	await page.getByRole('button', { name: 'Ajouter une catégorie' }).click();
	await expect(page.getByRole('status')).toContainText('Catégorie « Études » ajoutée.');
	expect(settings()).toContain('[categories.etudes]\nname = "Études"\nsensitive = false\n');
	expect(readList()).toContain('"etudes"');
	await expect(page.getByTestId('ai').getByRole('switch', { name: 'Claude Desktop peut lire : Études' })).toBeChecked();

	await page.getByLabel('Son nom', { exact: true }).last().fill('Thérapie');
	await page.getByRole('button', { name: 'Ajouter une catégorie' }).click();
	await expect(page.getByRole('status')).toContainText('Catégorie « Thérapie » ajoutée.');
	expect(settings()).toContain('[categories.therapie]\nname = "Thérapie"\nsensitive = true\n');
	expect(readList()).not.toContain('therapie');
	await expect(page.getByTestId('ai').getByRole('row', { name: /Thérapie/ })).toContainText('Sensible');
});

test('connect an AI, see its code once, and disconnect it', async ({ page }) => {
	await page.goto(ipse.url);
	await page.getByRole('navigation').getByRole('link', { name: 'IA connectées' }).click();
	await page.getByRole('button', { name: 'Connecter une IA' }).click();
	await page.getByLabel('LM Studio').check();
	await expect(page.getByLabel('Son nom', { exact: true }).first()).toHaveValue('LM Studio');
	await expect(page.getByLabel(/Elle tourne entièrement sur cet ordinateur/)).toBeChecked();
	await page.getByRole('button', { name: 'Connecter', exact: true }).click();

	await expect(page.getByRole('heading', { name: 'LM Studio est connectée' })).toBeFocused();
	const code = (await page.getByLabel('IPSE_TOKEN', { exact: true }).textContent())!;
	expect(code).toMatch(/^ipse_[0-9a-f]{64}$/);
	expect(clients()).not.toContain(code);
	expect(clients()).toContain('name = "LM Studio"');
	await expect(page.getByLabel('mcp.json', { exact: true })).toContainText(`"Authorization": "Bearer ${code}"`);
	await page.getByRole('button', { name: 'J’ai copié le code' }).click();
	await expect(page.getByLabel('IPSE_TOKEN', { exact: true })).toHaveCount(0);

	const lm = page.getByTestId('ai').filter({ has: page.getByRole('heading', { name: 'LM Studio' }) });
	await expect(lm).toContainText('Sur cet ordinateur');
	await lm.getByRole('button', { name: 'Déconnecter' }).click();
	await page.getByRole('dialog', { name: 'Déconnecter LM Studio ?' }).getByRole('button', { name: 'Déconnecter' }).click();
	await expect(page.getByRole('status')).toContainText('LM Studio est déconnectée');
	await expect(page.getByTestId('ai')).toHaveCount(1);
	expect(clients()).not.toContain('LM Studio');
});
