// The agenda's week and month views (issue #132), on the demo memory.
import { expect, test } from '@playwright/test';
import { startDemo, type Running } from './ipse.ts';

let ipse: Running;
test.beforeEach(async () => {
	ipse = await startDemo('fr');
});
test.afterEach(() => ipse.stop());

const monthTitle = (date: Date) => {
	const text = new Intl.DateTimeFormat('fr-FR', { month: 'long', year: 'numeric' }).format(date);
	return text.charAt(0).toUpperCase() + text.slice(1);
};

test('the month: today marked, earlier and later months, a day opens its week', async ({ page }) => {
	await page.goto(ipse.url);
	await page.getByRole('link', { name: 'Agenda' }).first().click();
	const views = page.getByRole('group', { name: 'Affichage de l’agenda' });
	await expect(views.getByRole('button', { name: 'Liste' })).toHaveAttribute('aria-pressed', 'true');

	await views.getByRole('button', { name: 'Mois' }).click();
	const now = new Date();
	await expect(page.getByRole('heading', { level: 2, name: monthTitle(now) })).toBeVisible();
	const grid = page.getByRole('table', { name: monthTitle(now) });
	await expect(grid.getByRole('columnheader')).toHaveCount(7);
	await expect(grid.getByRole('columnheader').first()).toContainText('lun.');
	const today = grid.locator('td[aria-current="date"]');
	await expect(today).toHaveCount(1);
	await expect(today.getByRole('link').first()).toHaveText(String(now.getDate()));
	// Two years of invented agenda: the month has events, with their time.
	await expect(grid.locator('.cell-events a').first()).toBeVisible();

	// The address keeps the view: a reload stays on the month.
	await page.reload();
	await expect(page.getByRole('heading', { level: 2, name: monthTitle(now) })).toBeVisible();

	await page.getByRole('button', { name: 'Mois suivant' }).click();
	const next = new Date(now.getFullYear(), now.getMonth() + 1, 1);
	await expect(page.getByRole('heading', { level: 2, name: monthTitle(next) })).toBeVisible();
	await expect(page.locator('td[aria-current="date"]')).toHaveCount(0);
	await page.getByRole('button', { name: 'Mois précédent' }).click();
	await page.getByRole('button', { name: 'Mois précédent' }).click();
	const before = new Date(now.getFullYear(), now.getMonth() - 1, 1);
	await expect(page.getByRole('heading', { level: 2, name: monthTitle(before) })).toBeVisible();
	await page.getByRole('button', { name: 'Aujourd’hui' }).click();
	await expect(page.getByRole('heading', { level: 2, name: monthTitle(now) })).toBeVisible();

	// Today's number opens its week.
	await page.locator('td[aria-current="date"] .day-number').click();
	await expect(views.getByRole('button', { name: 'Semaine' })).toHaveAttribute('aria-pressed', 'true');
	await expect(page.getByRole('heading', { level: 2, name: /^Semaine du lundi/ })).toBeVisible();
});

test('the week: seven days, each event with its time and kind, a day’s summary', async ({ page }) => {
	await page.goto(ipse.url);
	await page.getByRole('link', { name: 'Agenda' }).first().click();
	// From the keyboard.
	await page.getByRole('button', { name: 'Semaine' }).focus();
	await page.keyboard.press('Enter');
	const week = page.getByRole('list', { name: /^Semaine du lundi/ });
	await expect(week.locator(':scope > li')).toHaveCount(7);
	await expect(week.locator(':scope > li[aria-current="date"]')).toHaveCount(1);
	const events = week.locator('.week-events li');
	await expect(events.first()).toBeVisible();
	await expect(events.first().locator('.when')).toHaveText(/\d{2}:\d{2}|Toute la journée/);

	// Last week has the demo's day summaries.
	await page.getByRole('button', { name: 'Semaine précédente' }).click();
	await expect(week.getByRole('link', { name: 'Lire le résumé de la journée' }).first()).toBeVisible();
	// Four months ago, events have their kind.
	const past = new Date(Date.now() - 120 * 86_400_000).toISOString().slice(0, 10);
	await page.goto(`${ipse.origin}/agenda?view=week&day=${past}`);
	await expect(week.locator('.kind-tag').first()).toBeVisible();

	// An event opens its page.
	const first = week.locator('.week-events a').first();
	const name = (await first.textContent()) ?? '';
	await first.click();
	await expect(page.getByRole('heading', { level: 1 })).toHaveText(name.trim());
});

test('on a phone, the week is a list of days and the month counts events', async ({ page }) => {
	await page.setViewportSize({ width: 390, height: 844 });
	await page.goto(ipse.url);
	await page.goto(`${ipse.origin}/agenda?view=week`);
	const week = page.getByRole('list', { name: /^Semaine du lundi/ });
	const columns = await week.evaluate((el) => getComputedStyle(el).gridTemplateColumns.split(' ').length);
	expect(columns).toBe(1);
	await page.getByRole('button', { name: 'Mois' }).click();
	await expect(page.locator('.cell-events').first()).toBeHidden();
	await expect(page.locator('.cell-count').first()).toBeVisible();
});
