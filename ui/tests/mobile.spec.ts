import { expect, test } from '@playwright/test';
import { startDemo, type Running } from './ipse.ts';

// The compact menu on a phone (issue #140), at 390 px.
let ipse: Running;
test.use({ viewport: { width: 390, height: 844 } });
test.beforeEach(async () => {
	ipse = await startDemo('fr');
});
test.afterEach(() => ipse.stop());

test('every section one tap away, the search in the top bar', async ({ page }) => {
	await page.goto(ipse.url);
	const bar = page.getByRole('navigation', { name: 'Sections' });
	await expect(bar.getByRole('link')).toHaveText(['Accueil', 'Mémoire', 'Agenda', /À revoir/]);
	await expect(bar.getByRole('link', { name: /À revoir/ }).locator('.count')).toHaveAccessibleName('9 à revoir');
	await expect(bar.getByRole('link', { name: 'Accueil' })).toHaveAttribute('aria-current', 'page');
	// Nothing hides behind the bar: the end of the page scrolls above it.
	const box = await bar.boundingBox();
	expect(box!.y + box!.height).toBeLessThanOrEqual(844);

	for (const name of ['Mémoire', 'Agenda']) {
		await bar.getByRole('link', { name }).click();
		await expect(page.getByRole('heading', { level: 1 })).toHaveText(name);
		await expect(bar.getByRole('link', { name })).toHaveAttribute('aria-current', 'page');
	}
	await bar.getByRole('link', { name: /À revoir/ }).click();
	await expect(page.getByRole('heading', { level: 1 })).toHaveText('À revoir');

	// The others, behind More (the owner's initials): their file and the settings first,
	// focus goes in, a choice closes it.
	const more = bar.getByRole('button', { name: 'Plus' });
	await expect(more.locator('.face')).toHaveText('CL');
	await more.click();
	await expect(page.getByRole('dialog', { name: 'Autres sections' }).getByRole('link').first()).toHaveText('Votre fiche');
	await page.keyboard.press('Escape');
	for (const name of ['Réglages', 'Carte', 'Graphe', 'Résumés', 'IA connectées', 'Sources', 'Historique']) {
		await more.click();
		const panel = page.getByRole('dialog', { name: 'Autres sections' });
		await expect(panel).toBeVisible();
		await expect(more).toHaveAttribute('aria-expanded', 'true');
		await panel.getByRole('link', { name }).click();
		await expect(panel).toBeHidden();
		await expect(page.getByRole('heading', { level: 1 })).toHaveText(name);
		await expect(more).toHaveAccessibleDescription(`Vous êtes dans : ${name}`);
	}

	// Escape and a tap outside close it; focus comes back to More.
	await more.click();
	const panel = page.getByRole('dialog', { name: 'Autres sections' });
	await expect(panel.getByRole('link', { name: 'Historique' })).toBeFocused();
	await page.keyboard.press('Escape');
	await expect(panel).toBeHidden();
	await expect(more).toBeFocused();
	await more.click();
	await page.mouse.click(195, 60);
	await expect(panel).toBeHidden();
	await more.click();
	await panel.getByRole('button', { name: 'Fermer' }).click();
	await expect(panel).toBeHidden();
	await expect(more).toBeFocused();
	// The browser gives focus back to More as the panel closes, then the panel's close
	// event gives it again; aria-expanded turns false in that same handler. Waiting for it
	// keeps that second focus from landing after the search is filled (#163).
	await expect(more).toHaveAttribute('aria-expanded', 'false');

	// The search, from the top bar of any screen.
	const search = page.getByRole('searchbox', { name: 'Chercher dans la mémoire' });
	await search.fill('escalade');
	await search.press('Enter');
	await expect(page).toHaveURL(`${ipse.origin}/memory?q=escalade`);
	await expect(page.getByTestId('found').getByRole('link').first()).toContainText('Club d’escalade Vertige');
});

test('widening the window with More open leaves the page usable', async ({ page }) => {
	await page.setViewportSize({ width: 390, height: 844 });
	await page.goto(ipse.url);
	await page.getByRole('navigation', { name: 'Sections' }).last().getByRole('button', { name: 'Plus' }).click();
	const panel = page.getByRole('dialog', { name: 'Autres sections' });
	await expect(panel).toBeVisible();
	await page.setViewportSize({ width: 1280, height: 800 });
	await expect(panel).toBeHidden();
	await page.getByRole('link', { name: 'Agenda' }).first().click();
	await expect(page.getByRole('heading', { level: 1 })).toHaveText('Agenda');
});

test('the sources to add stack one under the other, and a form fits the screen', async ({ page }) => {
	await page.goto(ipse.url);
	await page.goto(new URL('/sources', ipse.url).href);
	const cards = page.getByRole('group', { name: 'Agendas et contacts' }).getByRole('link');
	await expect(cards).toHaveCount(3);
	const [first, second] = [await cards.nth(0).boundingBox(), await cards.nth(1).boundingBox()];
	expect(second!.y).toBeGreaterThan(first!.y + first!.height - 1);
	const wide = () => page.evaluate(() => document.documentElement.scrollWidth > window.innerWidth);
	expect(await wide()).toBe(false);
	await page.getByRole('link', { name: 'Home Assistant' }).click();
	await expect(page.getByRole('region', { name: 'Connecter Home Assistant' })).toBeVisible();
	expect(await wide()).toBe(false);
});
