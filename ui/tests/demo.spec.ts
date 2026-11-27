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
	// Yesterday's summary, written on the day the demo is made (issue #261).
	await expect(page.getByRole('region', { name: 'Dernier résumé' })).toContainText('Jours de sel');
	await expect(page.locator('.banner')).toContainText('6 propositions et 3 questions attendent votre avis');

	// Two duplicates found by Ipse, two summaries, a fact and kinds of events from Claude, and questions.
	await expect(sections.getByRole('link', { name: /À revoir/ }).locator('.count')).toHaveText('9');
	await sections.getByRole('link', { name: /À revoir/ }).click();
	await expect(page.getByTestId('proposal')).toHaveCount(6);
	await expect(page.getByTestId('question')).toHaveCount(3);
	await expect(page.getByTestId('decided').filter({ hasText: 'Refusée' })).toHaveCount(1);

	await sections.getByRole('link', { name: 'Mémoire' }).click();
	await page.getByTestId('kinds').getByRole('link', { name: /Personnes/ }).click();
	await expect(page.getByRole('link', { name: /Maman/ }).first()).toBeVisible();
	await sections.getByRole('link', { name: 'Résumés' }).click();
	await expect(page.getByTestId('summary').first()).toBeVisible();
});

test('the demo has a location history: the map, and where one meets people', async ({ page }) => {
	await page.goto(ipse.url);
	await page.getByRole('link', { name: 'Carte' }).first().click();
	const map = page.getByRole('application', { name: /Carte de vos lieux/ });
	await expect(page.locator('.map-list').getByRole('button', { name: 'Chez Jade' })).toBeVisible();
	await expect(map).toHaveAttribute('data-drawn', /^[1-9]/);
	await page.locator('.map-list').getByRole('button', { name: 'Chez Jade' }).click();
	await expect(page.locator('.map-side')).toContainText('Jade Petit');
	await page.locator('.map-side').getByRole('link', { name: 'Jade Petit' }).click();
	await expect(page.getByRole('heading', { name: 'Où vous vous voyez' })).toBeVisible();
	await expect(page.getByTestId('places').getByRole('link', { name: 'Chez Jade' })).toBeVisible();
});

test('the music of the moment opens its files', async ({ page }) => {
	await page.goto(ipse.url);
	await page.getByTestId('now').getByRole('link', { name: 'Les Ondes Basses' }).click();
	await expect(page.getByRole('heading', { level: 1 })).toHaveText('Les Ondes Basses');
});
