import { expect, test } from '@playwright/test';
import { startIpse, type Running } from './ipse.ts';

let ipse: Running;
test.beforeEach(async () => {
	ipse = await startIpse();
});
test.afterEach(() => ipse.stop());

test('the frame opens in the vault language, in under a second', async ({ page }) => {
	const requests: string[] = [];
	page.on('request', (request) => requests.push(request.url()));
	await page.goto(ipse.url);
	await expect(page.getByTestId('memory-name')).toHaveText('Inès Roux');
	const firstScreen = Date.now() - ipse.startedAt;
	console.log(`ipse ui to the first screen: ${firstScreen} ms`);
	expect(firstScreen).toBeLessThan(1000);

	await expect(page).toHaveURL(`${ipse.origin}/`);
	await expect(page).toHaveTitle('Inès Roux · Ipse');
	const nav = page.getByRole('navigation', { name: 'Sections' });
	await expect(nav.getByRole('link')).toHaveText(['À revoir', 'IA connectées', 'Historique']);
	await expect(nav.getByRole('link', { name: 'À revoir' })).toHaveAttribute('aria-current', 'page');
	await expect(page.getByRole('heading', { level: 1 })).toHaveText('À revoir');
	await expect(page.getByText('Rien à revoir pour le moment.')).toBeVisible();

	// Nothing leaves this computer.
	for (const url of requests) expect(new URL(url).origin).toBe(ipse.origin);
});

test('the sections and the language switch', async ({ page }) => {
	await page.goto(ipse.url);
	await page.getByRole('link', { name: 'Historique' }).click();
	await expect(page).toHaveURL(`${ipse.origin}/history`);
	await expect(page.getByRole('heading', { level: 1 })).toHaveText('Historique');

	await page.getByRole('button', { name: 'EN' }).click();
	await expect(page.locator('html')).toHaveAttribute('lang', 'en');
	await expect(page.getByRole('heading', { level: 1 })).toHaveText('History');
	await expect(page.getByRole('navigation').getByRole('link')).toHaveText(['To review', 'Connected AIs', 'History']);

	// The choice stays in this browser; a reload keeps the page and the language.
	await page.reload();
	await expect(page.getByRole('heading', { level: 1 })).toHaveText('History');
	await expect(page.getByRole('button', { name: 'EN' })).toHaveAttribute('aria-pressed', 'true');
});

test('the keyboard reaches every section', async ({ page }) => {
	await page.goto(ipse.url);
	await expect(page.getByTestId('memory-name')).toBeVisible();
	await page.keyboard.press('Tab');
	await page.keyboard.press('Tab');
	await expect(page.getByRole('link', { name: 'À revoir' })).toBeFocused();
	await page.keyboard.press('Tab');
	await page.keyboard.press('Enter');
	await expect(page).toHaveURL(`${ipse.origin}/ais`);
});

test('the address works once, in one browser', async ({ page, browser }) => {
	await page.goto(ipse.url);
	await expect(page.getByTestId('memory-name')).toBeVisible();
	const other = await browser.newPage();
	const response = await other.goto(ipse.url);
	expect(response?.status()).toBe(403);
	await expect(other.getByText('lancez ipse ui sur cet ordinateur')).toBeVisible();
	await other.close();
});
