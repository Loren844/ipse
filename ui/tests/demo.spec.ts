import { expect, test } from '@playwright/test';
import { startDemo, type Running } from './ipse.ts';

// The demo memory (`ipse init --demo`) opens on a full memory: something to see on every
// screen, without anyone's real data.
let ipse: Running;
test.beforeEach(async () => {
	ipse = await startDemo('fr');
});
test.afterEach(() => ipse.stop());

test('the demo memory has something on every screen', async ({ page }) => {
	await page.goto(ipse.url);
	await expect(page.getByTestId('memory-name')).toHaveText('Camille Laurent');
	const sections = page.getByRole('navigation', { name: 'Sections' });
	await expect(page.getByText('Dernier résumé')).toBeVisible();
	await expect(page.locator('.banner')).toContainText('5 propositions et 3 questions attendent votre avis');

	// Two duplicates found by Ipse, two summaries and a fact from Claude, and questions.
	await expect(sections.getByRole('link', { name: /À revoir/ }).locator('.count')).toHaveText('8');
	await sections.getByRole('link', { name: /À revoir/ }).click();
	await expect(page.getByTestId('proposal')).toHaveCount(5);
	await expect(page.getByTestId('question')).toHaveCount(3);
	await expect(page.getByTestId('decided').filter({ hasText: 'Refusée' })).toHaveCount(1);

	await sections.getByRole('link', { name: 'Personnes' }).click();
	await expect(page.getByRole('link', { name: /Maman/ }).first()).toBeVisible();
	await sections.getByRole('link', { name: 'Résumés' }).click();
	await expect(page.getByTestId('summary').first()).toBeVisible();
});
