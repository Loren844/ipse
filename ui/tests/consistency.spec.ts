import { expect, test, type Page } from '@playwright/test';
import { startDemo, type Running } from './ipse.ts';

// The small inconsistencies found on the demo memory (issue #260), in both languages:
// one count everywhere, labels that say what they show, no heading said twice, time
// zones a person can read.
let ipse: Running;
test.afterEach(() => ipse.stop());

const sections = (page: Page) => page.getByRole('navigation', { name: /Sections/ });

for (const [language, w] of [
	[
		'fr',
		{
			review: /À revoir/,
			counted: '6 propositions et 3 questions attendent votre avis',
			next: /^Voir les \d+ prochaines dates$/,
			list: /^Voir les \d+ dates de la liste$/,
			agenda: 'Agenda',
			select: 'Tout sélectionner',
			accept: 'Accepter les 6 propositions sélectionnées',
			acceptOne: 'Accepter la proposition sélectionnée',
			where: 'Où vous êtes'
		}
	],
	[
		'en',
		{
			review: /To review/,
			counted: '6 proposals and 3 questions wait for you',
			next: /^Show the next \d+ dates$/,
			list: /^Show the \d+ dates in this list$/,
			agenda: 'Agenda',
			select: 'Select all',
			accept: 'Accept the 6 selected proposals',
			acceptOne: 'Accept the selected proposal',
			where: 'Where you are'
		}
	]
] as const) {
	test.describe(language, () => {
		test.use({ locale: language === 'fr' ? 'fr-FR' : 'en-GB' });
		test.beforeEach(async () => {
			ipse = await startDemo(language);
		});

		test('what waits is counted the same in the side bar, at home and on the review page', async ({ page }) => {
			await page.goto(ipse.url);
			await expect(sections(page).getByRole('link', { name: w.review }).locator('.count')).toHaveText('9');
			await expect(page.locator('.banner')).toContainText(w.counted);
			await sections(page).getByRole('link', { name: w.review }).click();
			await expect(page.locator('.page-head .lead')).toHaveText(`${w.counted}.`);
		});

		test('no screen says the same heading twice', async ({ page }) => {
			await page.goto(ipse.url);
			for (const path of ['/', '/review']) {
				await page.goto(`${ipse.origin}${path}`);
				await expect(page.getByRole('heading', { level: 1 })).toBeVisible();
				const headings = (await page.getByRole('heading').allTextContents()).map((h) => h.trim());
				expect(headings, path).toEqual([...new Set(headings)]);
			}
		});

		test('accepting a selection is offered once something is selected, and counts it', async ({ page }) => {
			await page.goto(ipse.url);
			await sections(page).getByRole('link', { name: w.review }).click();
			const toolbar = page.locator('.toolbar');
			await expect(toolbar.getByRole('button')).toHaveCount(0);
			await page.getByTestId('proposal').first().getByRole('checkbox').check();
			await expect(toolbar.getByRole('button')).toHaveText(w.acceptOne);
			await page.getByLabel(w.select).check();
			await expect(toolbar.getByRole('button')).toHaveText(w.accept);
		});

		test('a series button says which dates it unfolds', async ({ page }) => {
			await page.goto(ipse.url);
			// At home, only what comes next.
			const home = page.locator('ol.thread').first();
			const buttons = home.locator('button.unfold');
			await expect(buttons.first()).toBeVisible();
			for (const text of await buttons.allTextContents()) expect(text.trim()).toMatch(w.next);
			// The agenda's list also holds past dates.
			await sections(page).getByRole('link', { name: w.agenda }).click();
			const listed = page.locator('ol.thread.agenda button.unfold');
			await expect(listed.first()).toBeVisible();
			expect((await listed.allTextContents()).some((t) => w.list.test(t.trim()))).toBe(true);
		});

		test('where you are says each day once', async ({ page }) => {
			await page.goto(ipse.url);
			const where = page.getByTestId('now').locator('div').filter({ hasText: w.where });
			await expect(where.locator('.when').first()).toBeVisible();
			const days = await where.locator('.when').allTextContents();
			expect(days.length).toBeGreaterThan(0);
			expect(days).toEqual([...new Set(days)]);
		});
	});
}

test('time zones: cities in the interface’s language, this computer’s first, a search', async ({ browser }) => {
	ipse = await startDemo('fr');
	const context = await browser.newContext({ locale: 'fr-FR', timezoneId: 'America/Toronto' });
	const page = await context.newPage();
	await page.goto(ipse.url);
	await page.goto(`${ipse.origin}/settings`);
	const zone = page.getByLabel('Fuseau horaire');
	const options = zone.locator('option');
	await expect(options.first()).toHaveText(/^Toronto · .+ \(cet ordinateur\)$/);
	await expect(options.first()).toHaveAttribute('value', 'America/Toronto');
	// The memory's zone, then the others; never an administrative name.
	await expect(options.nth(1)).toHaveAttribute('value', 'Europe/Paris');
	await expect(options.nth(1)).toHaveText(/^Paris · heure d.+Europe centrale/);
	expect(await zone.locator('option[value^="Etc/"]').count()).toBe(0);
	for (const text of (await options.allTextContents()).slice(0, 50)) expect(text).not.toContain('_');

	await page.getByPlaceholder('Chercher une ville').fill('zurich');
	// The zone chosen stays in the list, so the select still shows it.
	await expect(options).toHaveText([/^Paris/, /^Zurich/]);
	await zone.selectOption('Europe/Zurich');
	await expect(page.getByRole('button', { name: 'Enregistrer' })).toBeEnabled();
	await expect(page.getByText('Les mots de ces écrans')).toBeVisible();
	await context.close();
});
