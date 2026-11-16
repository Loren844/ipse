import { expect, test } from '@playwright/test';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { startIpse, type Running } from './ipse.ts';

// Summaries as the AI proposes them (docs/spec/vault.md §12), on the invented test calendar.
function propose(vault: string, id: string, period: string, start: string, file: string, text: string) {
	const proposal = {
		format: 1,
		id,
		by: 'client:claude',
		created: '2026-11-16T07:00:00+01:00',
		reason: 'Résumé.',
		categories: ['summaries'],
		changes: [{ kind: 'write_summary', file, period, start, text }],
		status: { state: 'waiting' }
	};
	mkdirSync(join(vault, '.ipse/proposals'), { recursive: true });
	writeFileSync(join(vault, `.ipse/proposals/${id}.json`), JSON.stringify(proposal));
}

let ipse: Running;
test.beforeEach(async () => {
	ipse = await startIpse('Inès Roux', 'fr', true, (vault) => {
		propose(vault, '01jd3k8m2p4q6r8t0v2w4y6z8a', 'day', '2026-11-14', 'summaries/day/2026/2026-11-14.md', 'Dîner chez Julie avec Marc.');
		propose(vault, '01jd3k9m2p4q6r8t0v2w4y6z8a', 'day', '2026-11-10', 'summaries/day/2026/2026-11-10.md', 'Yoga, puis le marché.');
		propose(vault, '01jd3kam2p4q6r8t0v2w4y6z8a', 'week', '2026-11-09', 'summaries/week/2026/2026-W46.md', 'Une semaine de yoga et d’amis.');
	});
});
test.afterEach(() => ipse.stop());

const file = (path: string) => readFileSync(join(ipse.vault, path), 'utf8');
const nav = (page: import('@playwright/test').Page, name: string) =>
	page.getByRole('navigation', { name: 'Sections' }).getByRole('link', { name, exact: true });

test('read the summaries, see a day in the agenda, correct one', async ({ page }) => {
	await page.goto(ipse.url);
	await nav(page, 'Résumés').click();
	await expect(page.getByRole('heading', { level: 1 })).toHaveText('Résumés');
	await expect(page.getByText('Aucun résumé pour l’instant.')).toBeVisible();
	await expect(page.locator('.empty-thread').getByRole('link', { name: 'IA connectées' })).toBeVisible();
	// Waiting in the inbox: pointed to, then accepted there.
	await page.getByRole('link', { name: /3 résumés attendent votre avis/ }).click();
	await page.getByLabel('Tout sélectionner').check();
	await page.getByRole('button', { name: 'Accepter les 3 propositions sélectionnées' }).click();
	await expect(page.getByRole('status')).toContainText('3 propositions acceptées.');

	await nav(page, 'Résumés').click();
	const summaries = page.getByTestId('summary');
	await expect(summaries.getByRole('heading', { level: 2 })).toHaveText(['Samedi 14 novembre', 'Mardi 10 novembre']);
	await expect(summaries.first()).toContainText('Dîner chez Julie avec Marc.');
	await expect(summaries.first().getByRole('link', { name: 'Dîner chez Julie' })).toBeVisible();

	await page.getByRole('link', { name: 'Semaines' }).click();
	await expect(page).toHaveURL(/period=week/);
	await expect(summaries.getByRole('heading', { level: 2 })).toHaveText(['Semaine 46 de 2026']);
	await expect(summaries).toContainText('Écrit à partir de');
	await summaries.getByRole('link', { name: 'Samedi 14 novembre' }).click();
	await expect(page).toHaveURL(/period=day&before=2026-11-14/);
	await expect(summaries.first().getByRole('heading')).toHaveText('Samedi 14 novembre');
	await page.getByRole('link', { name: 'Années' }).click();
	await expect(page.getByText('Pas encore de résumé pour cette durée')).toBeVisible();

	// Correct the week, from the keyboard: the person's own change.
	await page.getByRole('link', { name: 'Semaines' }).click();
	await summaries.getByRole('button', { name: 'Corriger — Semaine 46 de 2026' }).click();
	const field = page.getByLabel('Votre version du résumé : Semaine 46 de 2026');
	await expect(field).toBeFocused();
	await field.fill('Une semaine de yoga, d’amis et de pluie.');
	await page.getByRole('button', { name: 'Enregistrer' }).click();
	await expect(summaries.getByRole('status')).toContainText('Ce texte est maintenant le vôtre');
	await expect(summaries).toContainText('Une semaine de yoga, d’amis et de pluie.');
	expect(file('summaries/week/2026/2026-W46.md')).toContain('\n## Résumé\n\nUne semaine de yoga, d’amis et de pluie.\n');
	await nav(page, 'Historique').click();
	const last = page.getByTestId('changes').getByRole('listitem').first();
	await expect(last).toContainText('Vous');
	await expect(last).toContainText('Résumé corrigé');

	// A day's summary in the agenda, under its day; the latest on the home screen.
	await nav(page, 'Agenda').click();
	for (let i = 0; i < 40 && !(await page.getByText('Dîner chez Julie avec Marc.').count()); i++) {
		await page.getByRole('button', { name: 'Plus tôt' }).click();
		await page.waitForLoadState('networkidle');
	}
	await expect(page.locator('.day-label', { hasText: 'Samedi 14 novembre' }).locator('.day-summary')).toContainText(
		'Dîner chez Julie avec Marc.'
	);
	await nav(page, 'Accueil').click();
	await expect(page.getByRole('region', { name: 'Dernier résumé' })).toContainText('Dîner chez Julie avec Marc.');
});
