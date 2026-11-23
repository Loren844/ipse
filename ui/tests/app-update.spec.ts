import { expect, test } from '@playwright/test';
import { startIpse, type Running } from './ipse.ts';

// A new version of the desktop app (issue #156), as the interface shows it. The test
// plays the app: it sets what app/src/updates.rs gives the page, and answers the
// addresses the app answers itself.
let ipse: Running;
test.beforeEach(async () => {
	ipse = await startIpse('Inès Roux', 'fr');
});
test.afterEach(() => ipse.stop());

test('a new version is offered, said plainly, and installed through the app', async ({ page }) => {
	await page.addInitScript(() => {
		window.ipseApp = { version: '0.2.0' };
	});
	const asked: string[] = [];
	await page.route('**/app/update/*', async (route) => {
		asked.push(new URL(route.request().url()).pathname);
		await route.fulfill({ status: 204 });
	});
	await page.goto(ipse.url);
	await expect(page.getByRole('heading', { level: 1 })).toBeVisible();
	await expect(page.getByTestId('update')).toHaveCount(0);

	const offer = (formatChanges: boolean) =>
		page.evaluate((format_changes) => {
			window.ipseApp!.update = { version: '0.3.0', notes: '### Added\n\n- **Updates**: the app keeps itself up to date.', format_changes };
			window.dispatchEvent(new Event('ipse-app-update'));
		}, formatChanges);
	await offer(false);
	const update = page.getByRole('region', { name: 'Ipse 0.3.0 est disponible' });
	await expect(update).toBeVisible();
	await expect(update).toContainText('Ipse vérifie la signature');
	await expect(update).not.toContainText('change le format');
	await update.getByText('Ce qui change').click();
	await expect(update).toContainText('the app keeps itself up to date');

	// Later: the app stops showing it.
	await update.getByRole('button', { name: 'Plus tard' }).click();
	await expect.poll(() => asked).toEqual(['/app/update/later']);
	await page.evaluate(() => {
		window.ipseApp!.update = null;
		window.dispatchEvent(new Event('ipse-app-update'));
	});
	await expect(update).toHaveCount(0);

	// A version that changes the memory's format says so before it is installed.
	await offer(true);
	await expect(update).toContainText('Cette version change le format de votre mémoire.');
	await update.getByRole('button', { name: 'Installer et redémarrer' }).click();
	await expect(update.getByRole('button', { name: 'Installation…' })).toBeDisabled();
	await expect(update.getByRole('status')).toHaveText('Ipse redémarre dès que la nouvelle version est installée.');
	expect(asked).toEqual(['/app/update/later', '/app/update/install']);

	// If installing fails, the page says so, and the app goes on.
	await page.evaluate(() => {
		window.ipseApp!.update = { failed: true };
		window.dispatchEvent(new Event('ipse-app-update'));
	});
	await expect(page.getByText('La nouvelle version n’a pas pu s’installer.')).toBeVisible();
});

test('checking for updates is a setting of the app, when it can update itself', async ({ page }) => {
	await page.addInitScript(() => {
		window.ipseApp = { version: '0.2.0' };
	});
	const asked: string[] = [];
	await page.route('**/app/settings?*', async (route) => {
		const url = new URL(route.request().url());
		asked.push(url.search);
		await route.fulfill({ status: 204 });
		await page.evaluate((search) => {
			window.ipseApp!.settings!.updates = new URLSearchParams(search).get('updates') === 'on';
			window.dispatchEvent(new Event('ipse-app-settings'));
		}, url.search);
	});
	await page.goto(ipse.url);
	await page.goto(`${ipse.origin}/settings`);
	const updates = page.getByRole('checkbox', { name: /^Vérifier les mises à jour/ });
	// A build without the update key cannot update itself: no switch.
	await page.evaluate(() => {
		window.ipseApp!.settings = { notifications: true, start: false, updates: null };
		window.dispatchEvent(new Event('ipse-app-settings'));
	});
	await expect(page.getByRole('heading', { name: 'En arrière-plan' })).toBeVisible();
	await expect(updates).toHaveCount(0);

	await page.evaluate(() => {
		window.ipseApp!.settings = { notifications: true, start: false, updates: true };
		window.dispatchEvent(new Event('ipse-app-settings'));
	});
	await expect(updates).toBeChecked();
	await expect(page.getByText('Rien de votre mémoire n’est envoyé')).toBeVisible();
	await updates.uncheck();
	await expect(updates).not.toBeChecked();
	expect(asked).toEqual(['?updates=off']);
});
