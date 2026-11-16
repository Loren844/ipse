import { expect, test } from '@playwright/test';
import { startIpse, type Running } from './ipse.ts';

// A memory with the test contacts and calendar imported (invented people).
let ipse: Running;
test.beforeEach(async () => {
	ipse = await startIpse('Inès Roux', 'fr', true);
});
test.afterEach(() => ipse.stop());

test('search a person, open them, follow their event and come back', async ({ page }) => {
	await page.goto(ipse.url);
	await page.getByRole('link', { name: 'Personnes' }).click();
	await expect(page.getByRole('heading', { level: 1 })).toHaveText('Personnes');
	const list = page.getByTestId('people');
	await expect(list.getByRole('link')).toHaveText([/Élodie Martin/, /Inès Roux/, /Marc Leroy/]);

	// As one types, accents and case aside, by a nickname.
	await page.getByLabel('Chercher une personne').fill('lolo');
	await expect(list.getByRole('link')).toHaveText([/Élodie Martin/]);
	await expect(page).toHaveURL(/\?q=lolo$/);
	await page.getByLabel('Chercher une personne').fill('leroy');
	await expect(list.getByRole('link')).toHaveText([/Marc Leroy/]);
	await list.getByRole('link', { name: /Marc Leroy/ }).click();

	await expect(page.getByRole('heading', { level: 1 })).toHaveText('Marc Leroy');
	await expect(page).toHaveTitle('Marc Leroy · Ipse');
	await expect(page.getByRole('link', { name: 'Personnes' })).toHaveAttribute('aria-current', 'page');
	await expect(page.getByText('marc.leroy@example.org')).toBeVisible();
	await expect(page.getByText('Vient de vos contacts.')).toBeVisible();
	await expect(page.getByText(/\^f-/)).toHaveCount(0);

	await page.getByRole('link', { name: 'Dîner chez Julie' }).click();
	await expect(page.getByRole('heading', { level: 1 })).toHaveText('Dîner chez Julie');
	await expect(page.getByText('Julie Dupont')).toBeVisible();
	await page.getByRole('link', { name: /Marc Leroy/ }).click();
	await expect(page.getByRole('heading', { level: 1 })).toHaveText('Marc Leroy');
});

test('a person who does not exist', async ({ page }) => {
	await page.goto(ipse.url);
	await expect(page.getByTestId('memory-name')).toBeVisible();
	await page.goto(`${ipse.origin}/people/01zzzzzzzzzzzzzzzzzzzzzzzz`);
	await expect(page.getByRole('heading', { level: 1 })).toHaveText('Cette fiche n’existe pas, ou plus.');
});

test('the agenda goes back in time', async ({ page }) => {
	await page.goto(ipse.url);
	await page.getByRole('link', { name: 'Agenda' }).click();
	await expect(page.getByRole('heading', { level: 1 })).toHaveText('Agenda');
	// The test calendar is in November and December 2026: going back finds it, whatever today is.
	for (let i = 0; i < 40 && !(await page.getByRole('link', { name: 'Dîner chez Julie' }).count()); i++) {
		await page.getByRole('button', { name: 'Plus tôt' }).click();
		await page.waitForLoadState('networkidle');
	}
	await expect(page.getByRole('link', { name: 'Dîner chez Julie' })).toBeVisible();
});

test('the search box is one line, and a click anywhere in it lets you type', async ({ page }) => {
	await page.goto(ipse.url);
	for (const scheme of ['light', 'dark'] as const) {
		await page.emulateMedia({ colorScheme: scheme });
		await page.goto(`${ipse.origin}/people`);
		const field = page.locator('.search .field');
		const input = page.getByLabel('Chercher une personne');
		const box = (await field.boundingBox())!;
		const typed = (await input.boundingBox())!;
		const icon = (await field.locator('svg').boundingBox())!;
		expect(typed.width, scheme).toBeGreaterThan(box.width * 0.85);
		expect(Math.abs(icon.y + icon.height / 2 - (typed.y + typed.height / 2)), 'icon and input on one line').toBeLessThan(4);
		// Near the right end of the box, where the input used to be missing.
		await page.mouse.click(box.x + box.width - 30, box.y + box.height / 2);
		await page.keyboard.type('leroy');
		await expect(page.getByTestId('people').getByRole('link')).toHaveText([/Marc Leroy/]);
	}
});
