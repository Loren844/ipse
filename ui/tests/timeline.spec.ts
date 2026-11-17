// The timeline (issue #133), on the demo memory: years, their months, a month, then
// its calendar.
import { expect, test } from '@playwright/test';
import { startDemo, type Running } from './ipse.ts';

let ipse: Running;
test.beforeEach(async () => {
	ipse = await startDemo('fr');
});
test.afterEach(() => ipse.stop());

test('from the years to a month, then to its calendar', async ({ page }) => {
	await page.goto(ipse.url);
	await page.getByRole('link', { name: 'Frise' }).first().click();
	await expect(page.getByRole('heading', { level: 1 })).toHaveText('Frise');
	const thisYear = String(new Date().getFullYear());
	const years = page.locator('.year-row h2');
	await expect(years.first()).toHaveText(thisYear);
	// Each year: its count, a strip of its twelve months, and the people seen most.
	const year = page.locator('.year-row').first();
	await expect(year).toContainText(/\d+ événements/);
	await expect(year.getByRole('list', { name: `Les mois de ${thisYear}` }).getByRole('listitem')).toHaveCount(12);
	await expect(year.getByText('Le plus souvent avec')).toBeVisible();

	// The year: one card per month that has events.
	await year.getByRole('link', { name: thisYear, exact: true }).click();
	await expect(page.getByRole('heading', { level: 1 })).toHaveText(thisYear);
	await expect(page.getByRole('navigation', { name: 'Où vous êtes dans la frise' }).getByRole('link')).toHaveText(['Frise']);
	const marked = page.locator('.month-card').filter({ has: page.locator('.month-marks') }).first();
	const monthName = (await marked.locator('h2').textContent())?.trim() ?? '';
	await marked.getByRole('heading').getByRole('link').click();

	// A month: what marked it, with each event's kind, and who was there.
	await expect(page.getByRole('heading', { level: 1 })).toHaveText(`${monthName} ${thisYear}`);
	await expect(page.getByRole('heading', { name: 'Ce qui a marqué ce mois' })).toBeVisible();
	await expect(page.locator('.marks .kind-tag').first()).toHaveText(/Séjour|Fête/);
	await expect(page.getByRole('navigation', { name: 'Où vous êtes dans la frise' }).getByRole('link')).toHaveText(['Frise', thisYear]);

	// Then its calendar (the agenda's month view, #132), where a day opens its week.
	await page.getByRole('link', { name: 'Voir ce mois dans l’agenda' }).click();
	await expect(page).toHaveURL(/\/agenda\?view=month&day=\d{4}-\d{2}-01$/);

	// Back to the year from the month.
	await page.goBack();
	await page.getByRole('navigation', { name: 'Où vous êtes dans la frise' }).getByRole('link', { name: thisYear }).click();
	await expect(page.getByRole('heading', { level: 1 })).toHaveText(thisYear);
});

test('a month of the strip opens that month', async ({ page }) => {
	await page.goto(ipse.url);
	await page.goto(`${ipse.origin}/timeline`);
	const strip = page.locator('.year-row').first().locator('.strip');
	const link = strip.getByRole('link').first();
	const label = (await link.getAttribute('aria-label')) ?? '';
	await link.click();
	await expect(page.getByRole('heading', { level: 1 })).toHaveText(label.split(',')[0]);
});
