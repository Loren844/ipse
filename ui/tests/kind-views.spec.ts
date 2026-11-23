import { expect, test } from '@playwright/test';
import { startDemo, type Running } from './ipse.ts';

// Views by kind of event (issue #134), on the demo memory: the agenda filtered by kind,
// the page of a kind (who, where, how long), and the birthdays to come.
let ipse: Running;
test.beforeEach(async () => {
	ipse = await startDemo('fr');
});
test.afterEach(() => ipse.stop());

test('filter the agenda by kind, then open the page of that kind', async ({ page }) => {
	await page.goto(ipse.url);
	await page.getByRole('navigation', { name: 'Sections' }).getByRole('link', { name: 'Agenda' }).click();
	// The kinds the person gave are on events of more than three months ago.
	for (let i = 0; i < 2; i++) await page.getByRole('button', { name: 'Plus tôt' }).click();
	const filter = page.getByRole('group', { name: 'Montrer un type d’événement' });
	await filter.getByRole('button', { name: 'Repas' }).click();
	await expect(page).toHaveURL(/kind=meal/);
	const knots = page.locator('ol.agenda .knot');
	await expect(knots.first()).toBeVisible();
	expect(await knots.count()).toBeGreaterThan(0);
	await expect(knots.filter({ hasNot: page.locator('.kind-tag', { hasText: 'Repas' }) })).toHaveCount(0);

	// The month view keeps the filter.
	await page.getByRole('button', { name: 'Mois' }).click();
	await expect(page).toHaveURL(/kind=meal/);
	await expect(filter.getByRole('button', { name: 'Repas' })).toHaveAttribute('aria-pressed', 'true');
	await page.getByRole('button', { name: 'Liste' }).click();

	await page.getByRole('link', { name: 'Tout sur « Repas »' }).click();
	await expect(page.getByRole('heading', { level: 1 })).toHaveText('Repas');
	await expect(page.getByText('Avec qui vous partagez vos repas, et où.')).toBeVisible();
	await expect(page.getByText(/\d+ événements de ce type dans votre mémoire\./)).toBeVisible();
	await expect(page.getByTestId('with-whom').getByRole('listitem').first()).toContainText(/\d+ fois|une fois/);
	await expect(page.getByRole('heading', { name: 'Les derniers' })).toBeVisible();

	// Back to the agenda, still filtered.
	await page.locator('.crumbs').getByRole('link', { name: 'Agenda' }).click();
	await expect(page).toHaveURL(/kind=meal/);
});

test('a trip says how long it lasts; celebrations list the birthdays to come', async ({ page }) => {
	await page.goto(ipse.url);
	await page.goto(new URL('/agenda/kinds/trip', ipse.origin).href);
	await expect(page.getByRole('heading', { level: 1 })).toHaveText('Séjour');
	await expect(page.locator('.knot', { hasText: /\d+ jours/ }).first()).toBeVisible();

	// A kind tag opens its page.
	await page.locator('.knot .kind-tag').first().click();
	await expect(page).toHaveURL(/\/agenda\/kinds\/trip$/);

	await page.goto(new URL('/agenda/kinds/celebration', ipse.origin).href);
	await expect(page.getByRole('heading', { level: 1 })).toHaveText('Fête');
	const birthdays = page.getByTestId('birthdays').getByRole('listitem');
	await expect(birthdays.first()).toBeVisible();
	await expect(birthdays.filter({ hasText: /\d+ ans/ }).first()).toBeVisible();
	// The soonest first: a person's page opens from it.
	await birthdays.first().getByRole('link').click();
	await expect(page).toHaveURL(/\/memory\/people\//);
});

test('a person’s events can be shown one kind at a time', async ({ page }) => {
	await page.goto(ipse.url);
	// Someone the person travels with also shares meals with them.
	await page.goto(new URL('/agenda/kinds/trip', ipse.origin).href);
	await page.getByTestId('with-whom').getByRole('link').first().click();
	await expect(page).toHaveURL(/\/memory\/people\//);
	const filter = page.getByRole('group', { name: 'Montrer un type d’événement' });
	await filter.getByRole('button', { name: 'Repas' }).click();
	const knots = page.locator('ol.thread .knot');
	await expect(knots.first()).toBeVisible();
	await expect(knots.filter({ hasNot: page.locator('.kind-tag', { hasText: 'Repas' }) })).toHaveCount(0);
	await filter.getByRole('button', { name: 'Tous les types' }).click();
	await expect(knots.filter({ hasNot: page.locator('.kind-tag', { hasText: 'Repas' }) }).first()).toBeVisible();
});
