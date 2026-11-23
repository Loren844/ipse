import { expect, test, type Page } from '@playwright/test';
import { startDemo, type Running } from './ipse.ts';

// A little motion (issue #169), on the demo memory: what appears, leaves or changes
// place moves for 150 to 250 ms; nothing at all when the system asks for less motion.
let ipse: Running;
test.beforeEach(async () => {
	ipse = await startDemo('fr');
});
test.afterEach(() => ipse.stop());

const animation = (page: Page, selector: string) =>
	page.locator(selector).first().evaluate((el) => {
		const style = getComputedStyle(el);
		return `${style.animationName} ${style.animationDuration}`;
	});

test.describe('with motion', () => {
	test.use({ contextOptions: { reducedMotion: 'no-preference' } });

	test('screens, periods, menus and proposals move, briefly', async ({ page }) => {
		await page.goto(ipse.url);
		await page.getByRole('navigation', { name: 'Sections' }).getByRole('link', { name: 'Agenda' }).click();
		// Read once the new screen is in place: the old one, leaving, has no style left.
		await expect(page.getByRole('heading', { level: 1 })).toHaveText('Agenda');
		await expect.poll(() => animation(page, '#content > *')).toBe('appear 0.22s');
		await page.getByRole('button', { name: 'Semaine' }).click();
		await expect.poll(() => animation(page, '.appear')).toBe('fade-in 0.22s');

		// A menu drops in, and the next click is not held back.
		await page.getByRole('button', { name: /^Mémoire de/ }).click();
		await expect(page.getByRole('menu')).toBeVisible();
		await page.keyboard.press('Escape');
		await expect(page.getByRole('menu')).toHaveCount(0);

		// An accepted proposal leaves the list.
		await page.getByRole('navigation', { name: 'Sections' }).getByRole('link', { name: /À revoir/ }).click();
		const proposals = page.getByTestId('proposal');
		await expect(proposals.first()).toBeVisible();
		const before = await proposals.count();
		await proposals.first().getByRole('checkbox').check();
		const asked = Date.now();
		await page.getByRole('button', { name: /^Accepter/ }).first().click();
		await expect(proposals).toHaveCount(before - 1);
		expect(Date.now() - asked).toBeLessThan(2_000);
		await expect.poll(() => animation(page, '.notice')).toBe('appear 0.15s');
	});
});

test('nothing moves when the system asks for less motion', async ({ page }) => {
	await page.goto(ipse.url);
	await page.getByRole('navigation', { name: 'Sections' }).getByRole('link', { name: 'Agenda' }).click();
	await expect(page.getByRole('heading', { level: 1 })).toHaveText('Agenda');
	await page.getByRole('button', { name: 'Semaine' }).click();
	await expect(page.locator('.appear')).toHaveCount(1);
	await expect.poll(() => animation(page, '#content > *')).toMatch(/^none/);
	await expect.poll(() => animation(page, '.appear')).toMatch(/^none/);
});
