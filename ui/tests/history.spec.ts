import { expect, test } from '@playwright/test';
import { execFileSync } from 'node:child_process';
import { existsSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { IPSE, startIpse, type Running } from './ipse.ts';

const FIXTURES = resolve(import.meta.dirname, '../../crates/ipse/tests/fixtures');

/** The segment of the journal `source` wrote last. */
function lastSegment(dir: string): string {
	const found: string[] = [];
	const walk = (d: string) => {
		for (const e of readdirSync(d, { withFileTypes: true })) {
			if (e.isDirectory()) walk(join(d, e.name));
			else if (e.name.endsWith('.jsonl')) found.push(join(d, e.name));
		}
	};
	walk(dir);
	return found.sort().at(-1)!;
}

let ipse: Running;
test.beforeEach(async () => {
	ipse = await startIpse('Inès Roux', 'fr', true, (vault) => {
		// A crash in the middle of a write left a torn line; the next import cuts it off.
		const segment = lastSegment(join(vault, 'journal/contacts'));
		writeFileSync(segment, readFileSync(segment, 'utf8') + '{"id":"01torn');
		execFileSync(IPSE, ['import', join(FIXTURES, 'contacts/apple-v4.vcf'), '--vault', vault]);
		mkdirSync(join(vault, '.ipse/connectors/contacts'), { recursive: true });
		writeFileSync(join(vault, '.ipse/connectors/contacts/stderr.log'), 'reading 2 cards\nskipped a PHOTO field\n');
		execFileSync(IPSE, ['client', 'add', 'Claude Desktop', '--vault', vault]);
		mkdirSync(join(vault, '.ipse/access'), { recursive: true });
		const now = new Date();
		const month = now.toISOString().slice(0, 7);
		const call = (categories: string[], results: number) =>
			JSON.stringify({ at: now.toISOString(), client: 'claude-desktop', tool: 'search', categories, results }) + '\n';
		writeFileSync(join(vault, `.ipse/access/${month}.jsonl`), call(['people'], 3) + call(['people', 'agenda'], 2));
	});
});
test.afterEach(() => ipse.stop());

test('see who changed what, and undo an import', async ({ page }) => {
	await page.goto(ipse.url);
	await page.getByRole('navigation', { name: 'Sections' }).getByRole('link', { name: 'Historique' }).click();
	await expect(page.getByRole('heading', { level: 1 })).toHaveText('Historique');
	const changes = page.getByTestId('changes').getByRole('listitem');
	await expect(changes.first()).toContainText('Vos contacts');
	await expect(changes.first()).toContainText('Import de contacts');
	await expect(changes.first().getByRole('link', { name: 'Ana Silva' })).toBeVisible();
	expect(existsSync(join(ipse.vault, 'people/ana-silva.md'))).toBe(true);

	await changes.first().getByRole('button', { name: 'Annuler ce changement' }).click();
	await page.getByRole('dialog', { name: 'Annuler ce changement ?' }).getByRole('button', { name: 'Annuler ce changement' }).click();
	await expect(page.getByRole('status')).toContainText('Changement annulé : les fiches sont revenues comme avant.');
	expect(existsSync(join(ipse.vault, 'people/ana-silva.md'))).toBe(false);
	await expect(changes.first()).toContainText('Vous');
	await expect(changes.first()).toContainText('Changement annulé');
	// A removed file keeps its name, not its file name.
	await expect(changes.first()).toContainText('Ana Silva');
	await expect(changes.first()).not.toContainText('ana-silva');
	// The history keeps both.
	await expect(changes.nth(1)).toContainText('Import de contacts');
});

test('what the AIs read, and what the imports brought', async ({ page }) => {
	await page.goto(ipse.url);
	await page.getByRole('navigation', { name: 'Sections' }).getByRole('link', { name: 'Historique' }).click();
	await page.getByRole('link', { name: 'Accès des IA' }).click();
	await expect(page).toHaveURL(/view=accesses/);
	const accesses = page.getByTestId('accesses');
	await expect(accesses.getByRole('heading', { level: 3 })).toHaveText([/./]);
	await expect(accesses.getByRole('listitem')).toContainText(['Claude Desktop']);
	await expect(accesses).toContainText('2 appels, 5 réponses');
	await expect(accesses).toContainText('Personnes × 2');
	await expect(accesses).toContainText('Agenda × 1');

	await page.getByRole('link', { name: 'Imports et synchros' }).click();
	const runs = page.getByTestId('runs').locator('> li');
	await expect(runs).toHaveCount(3);
	await expect(runs.first()).toContainText('Vos contacts');
	await expect(runs.first()).toContainText('1 ligne du journal, laissée incomplète par un arrêt brutal, a été retirée');
	await expect(page.getByText('01torn')).toHaveCount(0);
	await expect(runs.nth(1)).toContainText('Votre agenda');
	await runs.first().getByRole('button', { name: 'Voir ses messages de diagnostic' }).click();
	await expect(runs.first().locator('pre')).toHaveText('reading 2 cards\nskipped a PHOTO field');
});
