import { expect, test } from '@playwright/test';
import { startDemo, type Running } from './ipse.ts';

// The view around someone (issue #244), on the demo memory: a close friend with
// something in every section, her years in the agenda, a place, and the graph one click
// away.
let ipse: Running;
test.afterEach(() => ipse.stop());

/** Opens the file of `name` from the memory's search. */
async function openFile(page: import('@playwright/test').Page, name: string) {
	await page.goto(`${ipse.origin}/memory?q=${encodeURIComponent(name)}`);
	await page.getByRole('link', { name: new RegExp(name) }).first().click();
	await expect(page.getByRole('heading', { level: 1 })).toHaveText(name);
}

test('everything around a person, by kind and by time', async ({ page }) => {
	ipse = await startDemo('fr');
	await page.goto(ipse.url);
	await openFile(page, 'Capucine Moreau');
	await page.getByRole('link', { name: 'Tout ce qui touche Capucine Moreau' }).click();
	await expect(page).toHaveURL(/\/around\?id=/);
	await expect(page.getByRole('heading', { level: 1 })).toHaveText('Capucine Moreau');
	await expect(page).toHaveTitle('Autour de Capucine Moreau · Ipse');
	// Reached from the memory: Mémoire stays the current section.
	await expect(page.getByRole('navigation', { name: 'Sections' }).getByRole('link', { name: 'Mémoire' })).toHaveAttribute('aria-current', 'page');

	// The header: who they are to the owner, how many events, when.
	await expect(page.getByTestId('around-what')).toHaveText('Personne · partenaire d’escalade');
	await expect(page.getByTestId('around-summary')).toContainText(/\d+ événements en tout, \d+ cette année/);
	await expect(page.getByTestId('around-summary')).toContainText(/Vus ensemble du .+ au .+/);

	// Each section, capped, with "see all".
	const events = page.getByTestId('around-events').getByRole('listitem');
	await expect(events).toHaveCount(5);
	const total = Number((await page.getByRole('heading', { name: /^Événements/ }).textContent())?.match(/\d+/)?.[0]);
	expect(total).toBeGreaterThan(5);
	await page.getByRole('button', { name: `Tout voir (${total})` }).click();
	await expect(events).toHaveCount(total);
	await expect(page.getByTestId('around-places').getByRole('link', { name: 'Chez Capucine' })).toBeVisible();
	await expect(page.getByTestId('around-places').getByText('Pas visible par vos IA').first()).toBeAttached();
	await expect(page.getByTestId('around-photos')).toContainText('37 photos dans Immich');
	await expect(page.getByTestId('around-facts')).toContainText('Prête volontiers ses livres.');
	await expect(page.getByTestId('around-reminders').getByRole('link', { name: 'Rendre son livre à Capucine' })).toBeVisible();
	await expect(page.getByRole('heading', { name: 'Souvent là aussi' })).toBeVisible();
	await expect(page.getByTestId('around-people').getByRole('listitem').first()).toContainText(/\d+ fois/);

	// Together through time: a bar opens the agenda's years on her events only.
	const bars = page.getByTestId('around-strip').getByRole('link');
	await expect(bars.first()).toBeVisible();
	await bars.last().click();
	await expect(page).toHaveURL(/\/agenda\?view=years&year=\d{4}&month=\d{2}&person=/);
	await expect(page.getByTestId('timeline-with')).toContainText('Capucine Moreau');
	await expect(page.locator('.seen').getByRole('link', { name: 'Capucine Moreau' })).toHaveCount(0);
	await page.getByTestId('timeline-with').getByRole('link', { name: 'Toute votre vie' }).click();
	await expect(page.getByTestId('timeline-with')).toHaveCount(0);

	// The graph, one click away.
	await page.goBack();
	await page.goBack();
	await page.getByRole('link', { name: 'Voir Capucine Moreau dans le graphe' }).click();
	await expect(page.getByTestId('graph-panel').getByRole('heading', { level: 2 })).toHaveText('Capucine Moreau');
});

test('around a place, on a phone, in English', async ({ page }) => {
	ipse = await startDemo('en');
	await page.setViewportSize({ width: 390, height: 844 });
	await page.goto(ipse.url);
	await openFile(page, 'Lotus Studio');
	await page.getByRole('link', { name: 'Everything about Lotus Studio' }).click();
	await expect(page.getByRole('heading', { level: 1 })).toHaveText('Lotus Studio');
	await expect(page.getByTestId('around-stays')).toContainText(/\d+ stays? in your history/);
	await expect(page.getByTestId('around-events').getByRole('listitem').first()).toContainText('Yoga');
	await expect(page.getByRole('heading', { name: 'What your AI remembers' })).toBeVisible();
	expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
});
