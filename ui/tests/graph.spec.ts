import { expect, test } from '@playwright/test';
import { startDemo, type Running } from './ipse.ts';

// The graph view (issue #130), on the demo memory.
let ipse: Running;
test.beforeEach(async () => {
	ipse = await startDemo('fr');
});
test.afterEach(() => ipse.stop());

test('the graph opens on the owner, recenters, filters, and reads as a list', async ({ page }) => {
	await page.goto(ipse.url);
	await page.getByRole('navigation', { name: 'Sections' }).getByRole('link', { name: 'Graphe' }).click();
	await expect(page.getByRole('heading', { level: 1 })).toHaveText('Graphe');
	await expect(page.getByText('Autour de Camille Laurent')).toBeVisible();
	await expect(page.getByTestId('graph').locator('.node')).not.toHaveCount(0);

	// The list holds the same neighbourhood, for the keyboard.
	const list = page.getByRole('region', { name: 'La même chose en liste' });
	await expect(list.getByRole('heading', { name: 'À un pas' })).toBeVisible();
	await expect(list.getByRole('listitem').filter({ has: page.getByRole('link', { name: 'Élise Laurent', exact: true }) }).first()).toContainText('sœur');
	await list.getByRole('button', { name: 'Mettre Élise Laurent au centre' }).first().click();
	await expect(page.getByText('Autour de Élise Laurent')).toBeVisible();
	await expect(page).toHaveURL(/\/graph\?id=/);

	// Filters: people only, then one step.
	const kinds = page.getByRole('group', { name: 'Montrer' });
	// Events are left out until asked for: there are many.
	await expect(kinds.getByRole('button', { name: 'Événements' })).toHaveAttribute('aria-pressed', 'false');
	await expect(list.getByRole('listitem').filter({ hasText: 'Déjeuner chez Maman' })).toHaveCount(0);
	await kinds.getByRole('button', { name: 'Événements' }).click();
	await expect(list.getByRole('listitem').filter({ hasText: 'Déjeuner chez Maman' }).first()).toContainText('Événement');
	for (const kind of ['Lieux', 'Organisations', 'Projets', 'Œuvres', 'Objets', 'Sujets', 'Événements'])
		await kinds.getByRole('button', { name: kind, exact: true }).click();
	await expect(kinds.getByRole('button', { name: 'Personnes', exact: true })).toBeDisabled();
	await expect(page).toHaveURL(/kinds=person/);
	await expect(list.getByRole('listitem').filter({ hasText: 'Événement' })).toHaveCount(0);
	await page.getByRole('button', { name: 'Un pas' }).click();
	await expect(list.getByRole('heading', { name: 'À deux pas' })).toHaveCount(0);

	// Clicking a file in the drawing recenters on it; the center opens it.
	await page.getByTestId('graph').locator('.node.depth-1').first().click();
	await expect(page.getByText('Autour de Élise Laurent')).toHaveCount(0);
	const name = (await page.getByTestId('graph').locator('.node.depth-0 text').textContent()) ?? '';
	await page.getByTestId('graph').locator('.node.depth-0').click();
	await expect(page.getByRole('heading', { level: 1 })).toHaveText(name.replace('…', ''));
});

test('a file opens its own graph, and a sensitive category is marked', async ({ page }) => {
	await page.goto(ipse.url);
	await page.goto(`${ipse.origin}/memory/places`);
	await page.getByRole('link', { name: 'Cabinet dentaire du Parc' }).click();
	await page.getByRole('link', { name: 'Voir Cabinet dentaire du Parc dans le graphe' }).click();
	await expect(page.getByText('Autour de Cabinet dentaire du Parc')).toBeVisible();
	const health = page.getByRole('group', { name: 'Catégories' }).getByRole('button', { name: /santé/ });
	await expect(health.getByRole('img', { name: 'Pas visible par vos IA' })).toBeVisible();
	await health.click();
	await expect(health).toHaveAttribute('aria-pressed', 'false');
	await expect(page).toHaveURL(/hide=health/);
});
