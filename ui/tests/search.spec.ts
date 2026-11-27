import { expect, test } from '@playwright/test';
import { startDemo, type Running } from './ipse.ts';

// Search that finds a memory (issue #258), on the demo memory: results say what they
// are, a series is one result, they are grouped by kind, and come while typing.
let ipse: Running;
test.afterEach(() => ipse.stop());

test('typing in the side bar shows a few results of each kind, Enter all of them', async ({ page }) => {
	ipse = await startDemo('fr');
	await page.goto(ipse.url);
	const box = page.getByRole('banner').getByRole('searchbox', { name: 'Chercher dans la mémoire' });
	await box.pressSequentially('yoga', { delay: 30 });
	const few = page.getByTestId('rail-found');
	await expect(few).toBeVisible();
	await expect(page).toHaveURL(`${ipse.origin}/`);

	// The yoga classes are one result, saying when they come back.
	await expect(few.getByText('Événements')).toBeVisible();
	const yoga = few.getByRole('link', { name: /^Yoga chaque mardi, 19:00 · \d+ fois depuis/ });
	await expect(yoga).toHaveCount(1);
	for (const list of await few.locator('ul').all()) expect(await list.getByRole('link').count()).toBeLessThanOrEqual(3);

	// Escape closes them; typing again opens them.
	await page.keyboard.press('Escape');
	await expect(few).toBeHidden();
	await expect(box).toBeFocused();
	await box.fill('');
	await box.pressSequentially('yoga', { delay: 30 });
	await expect(few).toBeVisible();

	// Enter: all of them, in Memory.
	await box.press('Enter');
	await expect(page).toHaveURL(`${ipse.origin}/memory?q=yoga`);
	await expect(few).toBeHidden();
	const found = page.getByTestId('found');
	const events = found.locator('section[data-kind="event"]');
	await expect(events.getByRole('heading', { name: /^Événements \d+$/ })).toBeVisible();
	const series = events.getByRole('link', { name: /^Yoga chaque mardi, 19:00 · \d+ fois depuis/ });
	await expect(series).toHaveCount(1);
	const unfold = events.getByRole('button', { name: /^Voir les \d+ dates$/ });
	const count = Number((await unfold.textContent())?.match(/\d+/)?.[0]);
	expect(count).toBeGreaterThan(5);
	await unfold.click();
	await expect(events.locator('.occurrences a')).toHaveCount(count);
	await events.getByRole('button', { name: 'Replier' }).click();
	await expect(events.locator('.occurrences')).toHaveCount(0);

	// The place says where the classes are, and the fact that matched.
	const lotus = found.locator('section[data-kind="place"]').getByRole('link', { name: /Studio Lotus/ });
	await expect(lotus.locator('mark')).toHaveText('yoga');
});

test('a person says what they are to the owner; a click on a result opens it', async ({ page }) => {
	ipse = await startDemo('fr');
	await page.goto(ipse.url);
	const box = page.getByRole('banner').getByRole('searchbox', { name: 'Chercher dans la mémoire' });
	await box.pressSequentially('capucine', { delay: 30 });
	const friend = page.getByTestId('rail-found').getByRole('link', { name: /^Capucine Moreau/ });
	await expect(friend).toContainText('partenaire d’escalade');
	await friend.click();
	await expect(page.getByRole('heading', { level: 1 })).toHaveText('Capucine Moreau');
	await expect(page.getByTestId('rail-found')).toBeHidden();
	await expect(box).toHaveValue('');
});

test('in English, dark, at phone width', async ({ browser }) => {
	ipse = await startDemo('en');
	const context = await browser.newContext({ locale: 'en-GB', colorScheme: 'dark', viewport: { width: 390, height: 844 } });
	const page = await context.newPage();
	await page.goto(ipse.url);
	const box = page.getByRole('searchbox', { name: 'Search your memory' });
	await box.pressSequentially('yoga', { delay: 30 });
	const few = page.getByTestId('rail-found');
	await expect(few.getByRole('link', { name: /^Yoga every Tuesday, 19:00 · \d+ times since/ })).toHaveCount(1);
	const area = await few.boundingBox();
	expect(area!.x).toBeGreaterThanOrEqual(0);
	expect(area!.x + area!.width).toBeLessThanOrEqual(390);
	expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(390);
	await expect(few.getByRole('link', { name: /^See all \d+ results in Memory$/ })).toBeVisible();
	await context.close();
});
