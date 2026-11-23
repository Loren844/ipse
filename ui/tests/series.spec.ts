import { expect, test } from '@playwright/test';
import { startDemo, type Running } from './ipse.ts';

// Repeating events (issue #175): yoga every Tuesday and the Monday team meeting of the
// demo memory show once in lists, unfold, and stay whole in the week and month views.
let ipse: Running;
test.beforeEach(async () => {
	ipse = await startDemo('fr');
});
test.afterEach(() => ipse.stop());

test('home and the agenda list show a series once, and unfold it', async ({ page }) => {
	await page.goto(ipse.url);
	const home = page.locator('ol.thread').first();
	const yoga = home.locator('.knot').filter({ has: page.getByRole('link', { name: 'Yoga', exact: true }) });
	await expect(yoga).toHaveCount(1);
	await expect(yoga).toContainText('chaque mardi, 19:00');
	await expect(home.locator('.knot').filter({ hasText: 'Point d’équipe' })).toHaveCount(1);
	await yoga.getByRole('button', { name: /Voir les \d+/ }).click();
	await expect(home.getByRole('link', { name: 'Yoga', exact: true }).nth(1)).toBeVisible();
	await home.getByRole('button', { name: 'Replier' }).first().click();
	await expect(home.getByRole('link', { name: 'Yoga', exact: true })).toHaveCount(1);

	await page.getByRole('navigation', { name: 'Sections' }).getByRole('link', { name: 'Agenda' }).click();
	const list = page.locator('ol.thread.agenda');
	await expect(list.getByRole('link', { name: 'Yoga', exact: true })).toHaveCount(1);
	const series = list.locator('.knot').filter({ hasText: 'chaque mardi, 19:00' });
	await expect(series).toHaveCount(1);
	const button = series.getByRole('button', { name: /Voir les \d+/ });
	const count = Number((await button.textContent())?.match(/\d+/)?.[0]);
	expect(count).toBeGreaterThan(5);
	await button.click();
	await expect(list.getByRole('link', { name: 'Yoga', exact: true })).toHaveCount(count);

	// The month keeps every occurrence: that is its job.
	await page.getByRole('button', { name: 'Mois' }).click();
	await expect(page.getByRole('link', { name: '19:00 Yoga' }).nth(3)).toBeVisible();
});

test('a person and a place show a series once; it still counts among who one sees most', async ({ page }) => {
	await page.goto(ipse.url);
	await page.goto(`${ipse.origin}/memory/places`);
	await page.getByRole('link', { name: 'Studio Lotus' }).click();
	const thread = page.locator('ol.thread');
	await expect(thread.getByRole('link', { name: 'Yoga', exact: true })).toHaveCount(1);
	await expect(thread).toContainText('chaque mardi, 19:00');

	await page.goto(`${ipse.origin}/memory/people`);
	await page.getByRole('link', { name: 'Elsa Girard', exact: true }).click();
	await expect(page.locator('ol.thread').getByRole('link', { name: 'Point d’équipe' })).toHaveCount(1);
	await expect(page.locator('ol.thread')).toContainText('chaque lundi, 09:30');

	await page.getByRole('navigation', { name: 'Sections' }).getByRole('link', { name: 'Frise' }).click();
	await expect(page.locator('.seen').first().getByRole('link', { name: /Elsa Girard/ })).toBeVisible();
});
