// The timeline (issue #133), the agenda's Years view since #211, on the demo memory:
// years, their months, a month, then its calendar and back.
import { expect, test } from '@playwright/test';
import { startDemo, type Running } from './ipse.ts';

let ipse: Running;
test.beforeEach(async () => {
	ipse = await startDemo('fr');
});
test.afterEach(() => ipse.stop());

test('from the years to a month, then to its calendar and back', async ({ page }) => {
	await page.goto(ipse.url);
	await page.getByRole('navigation', { name: 'Sections' }).getByRole('link', { name: 'Agenda' }).click();
	await page.getByRole('group', { name: 'Affichage de l’agenda' }).getByRole('button', { name: 'Années' }).click();
	await expect(page).toHaveURL(/\/agenda\?view=years$/);
	await expect(page.getByRole('heading', { level: 1 })).toHaveText('Agenda');
	await expect(page.getByRole('group', { name: 'Affichage de l’agenda' }).getByRole('button', { name: 'Années' })).toHaveAttribute('aria-pressed', 'true');
	const thisYear = String(new Date().getFullYear());
	const years = page.locator('.year-row h3');
	await expect(years.first()).toHaveText(thisYear);
	// Each year: its count, a strip of its twelve months, and the people seen most.
	const year = page.locator('.year-row').first();
	await expect(year).toContainText(/\d+ événements/);
	await expect(year.getByRole('list', { name: `Les mois de ${thisYear}` }).getByRole('listitem')).toHaveCount(12);
	await expect(year.getByText('Le plus souvent avec')).toBeVisible();

	// The year: one card per month that has events.
	await year.getByRole('link', { name: thisYear, exact: true }).click();
	const title = page.locator('.timeline-head h2');
	await expect(title).toHaveText(thisYear);
	await expect(page.getByRole('navigation', { name: 'Où vous êtes dans la frise' }).getByRole('link')).toHaveText(['Années']);
	const marked = page.locator('.month-card').filter({ has: page.locator('.month-marks') }).first();
	const monthName = (await marked.locator('h3').textContent())?.trim() ?? '';
	await marked.getByRole('heading').getByRole('link').click();

	// A month: what marked it, with each event's kind, and who was there.
	await expect(title).toHaveText(`${monthName} ${thisYear}`);
	await expect(page.getByRole('heading', { name: 'Ce qui a marqué ce mois' })).toBeVisible();
	await expect(page.locator('.marks .kind-tag').first()).toHaveText(/Séjour|Fête/);
	await expect(page.getByRole('navigation', { name: 'Où vous êtes dans la frise' }).getByRole('link')).toHaveText(['Années', thisYear]);

	// Then its calendar, the Month view, and back to what marked it.
	await page.getByRole('link', { name: 'Voir ce mois dans l’agenda' }).click();
	await expect(page).toHaveURL(/\/agenda\?view=month&day=\d{4}-\d{2}-01$/);
	await expect(page.getByRole('group', { name: 'Affichage de l’agenda' }).getByRole('button', { name: 'Mois' })).toHaveAttribute('aria-pressed', 'true');
	await page.getByRole('link', { name: 'Ce qui a marqué ce mois' }).click();
	await expect(title).toHaveText(`${monthName} ${thisYear}`);

	// Back to the year from the month.
	await page.getByRole('navigation', { name: 'Où vous êtes dans la frise' }).getByRole('link', { name: thisYear }).click();
	await expect(title).toHaveText(thisYear);
});

test('a month of the strip opens that month', async ({ page }) => {
	await page.goto(ipse.url);
	await page.goto(`${ipse.origin}/agenda?view=years`);
	const strip = page.locator('.year-row').first().locator('.strip');
	const link = strip.getByRole('link').first();
	const label = (await link.getAttribute('aria-label')) ?? '';
	await link.click();
	await expect(page.locator('.timeline-head h2')).toHaveText(label.split(',')[0]);
});

test('the old address of the timeline leads to the Years view, at the same month', async ({ page }) => {
	await page.goto(ipse.url);
	const thisYear = new Date().getFullYear();
	await page.goto(`${ipse.origin}/timeline?year=${thisYear}&month=01`);
	await expect(page).toHaveURL(new RegExp(`/agenda\\?view=years&year=${thisYear}&month=01$`));
});

test('most often with leaves out the weekly team meeting, unless work is counted', async ({ page }) => {
	await page.goto(ipse.url);
	await page.goto(`${ipse.origin}/agenda?view=years`);
	const counts = page.locator('.year-row').first().locator('.seen-count');
	await expect(counts.first()).toBeVisible();
	const without = await page.locator('.year-row').first().locator('.seen').textContent();
	const box = page.getByLabel('Compter aussi le travail');
	await expect(box).not.toBeChecked();
	await box.check();
	await expect(page).toHaveURL(/work=1/);
	// The colleagues of every Monday are back on top, seen more than anyone else.
	await expect(page.locator('.year-row').first().locator('.seen')).not.toHaveText(without ?? '');
	const top = Number(await counts.first().textContent());
	expect(top).toBeGreaterThan(30);
	await box.uncheck();
	await expect(page).not.toHaveURL(/work=1/);
	await expect(page.locator('.year-row').first().locator('.seen')).toHaveText(without ?? '');
});
