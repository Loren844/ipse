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
	// From starting `ipse ui` to the frame on screen: the server's start, then the page's
	// load in a browser already open (the browser's own start is not Ipse's).
	const asked = Date.now();
	await page.goto(ipse.url);
	await expect(page.getByTestId('memory-name')).toHaveText('Inès Roux');
	const firstScreen = ipse.startup + (Date.now() - asked);
	console.log(`ipse ui to the first screen: ${firstScreen} ms`);
	expect(firstScreen).toBeLessThan(1000);

	await expect(page).toHaveURL(`${ipse.origin}/`);
	await expect(page).toHaveTitle('Inès Roux · Ipse');
	const nav = page.getByRole('navigation', { name: 'Sections' });
	await expect(nav.getByRole('link')).toHaveText(['Accueil', 'Mémoire', 'Graphe', 'Agenda', 'Frise', 'Résumés', 'À revoir', 'IA connectées', 'Historique', 'Réglages']);
	await expect(nav.getByRole('link', { name: 'Accueil' })).toHaveAttribute('aria-current', 'page');
	await expect(page.getByRole('heading', { level: 1 })).toHaveText('Ce qui vient');
	await expect(page.getByText('Rien de prévu dans les 30 prochains jours.')).toBeVisible();

	// Nothing leaves this computer.
	for (const url of requests) expect(new URL(url).origin).toBe(ipse.origin);
});

test('the sections and the language switch', async ({ page }) => {
	await page.goto(ipse.url);
	await page.getByRole('link', { name: 'Historique' }).click();
	await expect(page).toHaveURL(`${ipse.origin}/history`);
	await expect(page.getByRole('heading', { level: 1 })).toHaveText('Historique');

	await page.getByRole('link', { name: 'Réglages' }).click();
	await page.getByRole('group', { name: 'Langue de l’interface' }).getByRole('button', { name: 'English' }).click();
	await expect(page.locator('html')).toHaveAttribute('lang', 'en');
	await expect(page.getByRole('heading', { level: 1 })).toHaveText('Settings');
	await expect(page.getByRole('navigation', { name: 'Sections' }).getByRole('link')).toHaveText([
		'Home',
		'Memory',
		'Graph',
		'Agenda',
		'Timeline',
		'Summaries',
		'To review',
		'Connected AIs',
		'History',
		'Settings'
	]);

	// The choice stays in this browser; a reload keeps the page and the language.
	await page.reload();
	await expect(page.getByRole('heading', { level: 1 })).toHaveText('Settings');
	await expect(page.getByRole('button', { name: 'English' })).toHaveAttribute('aria-pressed', 'true');
});

test('the keyboard reaches every section', async ({ page }) => {
	await page.goto(ipse.url);
	await expect(page.getByTestId('memory-name')).toBeVisible();
	await page.keyboard.press('Tab');
	await page.keyboard.press('Tab');
	await expect(page.getByRole('searchbox', { name: 'Chercher dans la mémoire' })).toBeFocused();
	await page.keyboard.press('Tab');
	await expect(page.getByRole('link', { name: 'Accueil' })).toBeFocused();
	await page.keyboard.press('Tab');
	await page.keyboard.press('Enter');
	await expect(page).toHaveURL(`${ipse.origin}/memory`);
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
