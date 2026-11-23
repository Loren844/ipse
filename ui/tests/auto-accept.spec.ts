import { expect, test } from '@playwright/test';
import { startDemo, type Running } from './ipse.ts';

// Accepting one kind of change from one AI in advance (issue #173, decision 0017): the
// demo's Claude had its last summaries accepted one by one, so Ipse offers it; the
// person turns it on or off per AI.
let ipse: Running;
test.beforeEach(async () => {
	ipse = await startDemo('fr');
});
test.afterEach(() => ipse.stop());

test('offered after summaries accepted in a row, then a switch per AI', async ({ page }) => {
	await page.goto(ipse.url);
	await page.getByRole('navigation', { name: 'Sections' }).getByRole('link', { name: /À revoir/ }).click();
	const offer = page.getByTestId('offer');
	await expect(offer).toHaveCount(1);
	await expect(offer.getByRole('heading')).toHaveText('Accepter les prochains résumés de Claude automatiquement ?');
	await expect(offer).toContainText('s’annulera en un clic');
	await offer.getByRole('button', { name: 'Oui, automatiquement' }).click();
	await expect(page.getByRole('status')).toContainText('Les résumés de Claude sont maintenant acceptés automatiquement.');
	await expect(offer).toHaveCount(0);

	await page.getByRole('navigation', { name: 'Sections' }).getByRole('link', { name: 'IA connectées' }).click();
	const claude = page.getByTestId('ai').filter({ hasText: 'Claude' });
	await expect(claude.getByRole('heading', { name: 'Accepter automatiquement' })).toBeVisible();
	await expect(claude).toContainText('Jamais pour les personnes, les faits ou les liens.');
	const summaries = claude.getByRole('switch', { name: 'Accepter automatiquement les résumés de Claude' });
	const kinds = claude.getByRole('switch', { name: 'Accepter automatiquement les types d’événements de Claude' });
	await expect(summaries).toBeChecked();
	await expect(kinds).not.toBeChecked();
	await kinds.click();
	await expect(kinds).toBeChecked();
	await summaries.click();
	await expect(summaries).not.toBeChecked();
	await expect(page.getByRole('status')).toContainText('Les résumés de Claude vous attendront de nouveau.');

	// Turned off: not offered again.
	await page.getByRole('navigation', { name: 'Sections' }).getByRole('link', { name: /À revoir/ }).click();
	await expect(page.getByTestId('proposal').first()).toBeVisible();
	await expect(page.getByTestId('offer')).toHaveCount(0);
});

test('no thanks is remembered', async ({ page }) => {
	await page.goto(`${ipse.url}`);
	await page.getByRole('navigation', { name: 'Sections' }).getByRole('link', { name: /À revoir/ }).click();
	await page.getByTestId('offer').getByRole('button', { name: 'Non merci' }).click();
	await expect(page.getByRole('status')).toContainText('Ipse ne le proposera plus.');
	await page.reload();
	await expect(page.getByTestId('proposal').first()).toBeVisible();
	await expect(page.getByTestId('offer')).toHaveCount(0);
	await page.getByRole('navigation', { name: 'Sections' }).getByRole('link', { name: 'IA connectées' }).click();
	await expect(page.getByRole('switch', { name: 'Accepter automatiquement les résumés de Claude' })).not.toBeChecked();
});
