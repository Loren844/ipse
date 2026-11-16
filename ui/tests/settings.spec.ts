import { expect, test, type Page } from '@playwright/test';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { startIpse, type Running } from './ipse.ts';

let ipse: Running;
test.beforeEach(async () => {
	ipse = await startIpse('Inès Roux', 'fr');
});
test.afterEach(() => ipse.stop());

const ground = (page: Page) => page.evaluate(() => getComputedStyle(document.body).backgroundColor);

test('choose the theme: system, light or dark, kept in this browser', async ({ page }) => {
	await page.emulateMedia({ colorScheme: 'light' });
	await page.goto(ipse.url);
	await page.getByRole('link', { name: 'Réglages' }).click();
	await expect(page.getByRole('heading', { level: 1 })).toHaveText('Réglages');
	const themes = page.getByRole('group', { name: 'Thème' });
	await expect(themes.getByRole('button', { name: 'Comme le système' })).toHaveAttribute('aria-pressed', 'true');
	await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
	const light = await ground(page);

	// The system turns dark: the page follows.
	await page.emulateMedia({ colorScheme: 'dark' });
	await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
	expect(await ground(page)).not.toBe(light);

	// Light, whatever the system says; kept after a reload, and set before the page paints.
	await themes.getByRole('button', { name: 'Clair' }).click();
	await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
	await page.reload({ waitUntil: 'domcontentloaded' });
	expect(await page.locator('html').getAttribute('data-theme')).toBe('light');
	await expect(themes.getByRole('button', { name: 'Clair' })).toHaveAttribute('aria-pressed', 'true');
	await themes.getByRole('button', { name: 'Sombre' }).click();
	await page.emulateMedia({ colorScheme: 'light' });
	await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
});

/** WCAG contrast ratio of two `rgb()` colours. */
function contrast(a: string, b: string): number {
	const lum = (c: string) => {
		const [r, g, bl] = c.match(/\d+(\.\d+)?/g)!.slice(0, 3).map(Number).map((v) => {
			const s = v / 255;
			return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
		});
		return 0.2126 * r + 0.7152 * g + 0.0722 * bl;
	};
	const [x, y] = [lum(a), lum(b)].sort((p, q) => q - p);
	return (x + 0.05) / (y + 0.05);
}

test('both themes are readable', async ({ page }) => {
	await page.goto(ipse.url);
	await page.getByRole('link', { name: 'Réglages' }).click();
	for (const name of ['Clair', 'Sombre']) {
		await page.getByRole('group', { name: 'Thème' }).getByRole('button', { name }).click();
		const colours = await page.evaluate(() => {
			const probe = document.createElement('span');
			document.body.append(probe);
			const read = (variable: string) => {
				probe.style.color = `var(${variable})`;
				return getComputedStyle(probe).color;
			};
			const out = Object.fromEntries(
				['--ground', '--surface', '--text', '--muted', '--thread', '--thread-ink', '--on-thread', '--added', '--warn'].map((v) => [v, read(v)])
			);
			probe.remove();
			return out;
		});
		const pairs: [string, string, number][] = [
			['--text', '--ground', 7],
			['--text', '--surface', 7],
			['--muted', '--ground', 4.5],
			['--muted', '--surface', 4.5],
			['--thread-ink', '--surface', 4.5],
			['--on-thread', '--thread', 4.5],
			['--added', '--surface', 4.5],
			['--warn', '--surface', 4.5],
			['--thread', '--surface', 3]
		];
		for (const [fg, bg, min] of pairs)
			expect(contrast(colours[fg], colours[bg]), `${name}: ${fg} on ${bg}`).toBeGreaterThanOrEqual(min);
	}
});

test('the memory’s language and time zone, saved as the person’s change', async ({ page }) => {
	await page.goto(ipse.url);
	await page.getByRole('link', { name: 'Réglages' }).click();
	await expect(page.getByTestId('version')).toHaveText(/^Ipse \d+\.\d+\.\d+/);
	await expect(page.getByText(ipse.vault)).toBeVisible();
	const save = page.getByRole('button', { name: 'Enregistrer' });
	await expect(save).toBeDisabled();
	await page.getByLabel('Fuseau horaire').selectOption('America/Toronto');
	await page.getByLabel('Langue des fiches').selectOption('en');
	await save.click();
	await expect(page.getByRole('status')).toContainText('Réglages enregistrés');
	const toml = readFileSync(join(ipse.vault, '.ipse/vault.toml'), 'utf8');
	expect(toml).toContain('language = "en"');
	expect(toml).toContain('time_zone = "America/Toronto"');
	// The interface keeps the person's chosen language; only the files change.
	await expect(page.getByRole('heading', { level: 1 })).toHaveText('Réglages');
	await page.getByRole('link', { name: 'Historique' }).click();
	await expect(page.getByText('Réglages modifiés')).toBeVisible();
});
