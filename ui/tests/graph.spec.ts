import { expect, test } from '@playwright/test';
import { startDemo, type Running } from './ipse.ts';

// The graph centred on one file (issue #213), on the demo memory: one click from the view
// around a file (issue #244), the owner first, a click recentres, a trail goes back, and no event is a node.
let ipse: Running;
test.beforeEach(async () => {
	ipse = await startDemo('fr');
});
test.afterEach(() => ipse.stop());

test('the graph opens on the owner, recentres on a click, and goes back by the trail', async ({ page }) => {
	await page.goto(ipse.url);
	// No longer in the menu (issue #244).
	await expect(page.getByRole('navigation', { name: 'Sections' }).getByRole('link', { name: 'Graphe' })).toHaveCount(0);
	await page.goto(`${ipse.origin}/graph`);
	await expect(page.getByRole('heading', { level: 1 })).toHaveText('Graphe');
	const panel = page.getByTestId('graph-panel');
	const drawing = page.getByTestId('graph');
	await expect(panel.getByRole('heading', { level: 2 })).toHaveText('Vous');

	// At most 30 neighbours at first, the strongest first; more on demand.
	const links = page.getByTestId('graph-links').getByRole('listitem');
	await expect(links).toHaveCount(30);
	await expect(drawing).toHaveAttribute('data-files', '31');
	const total = Number((await panel.getByRole('heading', { level: 3 }).first().textContent())?.match(/\d+/)?.[0]);
	expect(total).toBeGreaterThan(30);
	await panel.getByRole('button', { name: /Afficher \d+ liens? de plus/ }).click();
	await expect(links).toHaveCount(Math.min(60, total));

	// Each link in words: how often, and the relations written in the files.
	await expect(links.first()).toContainText(/\d+ événements avec vous sur 12 mois, le dernier le/);
	await expect(links.filter({ hasText: 'Maman' })).toContainText('mère');
	// No event is a node.
	await expect(drawing.locator('.ego-node')).toHaveCount(Math.min(60, total));
	await expect(links.filter({ hasText: 'Yoga' })).toHaveCount(0);

	// Hovering a neighbour lights its line and says the link under the drawing.
	await links.filter({ hasText: 'Maman' }).getByRole('button').hover();
	await expect(page.locator('.graph-caption')).toContainText('Maman');
	await expect(drawing.locator('.ego-link.lit')).toHaveCount(1);

	// The keyboard: the list, Enter recentres.
	const first = links.first().getByRole('button');
	const name = (await first.locator('.name').textContent())!;
	await first.focus();
	await page.keyboard.press('Enter');
	await expect(panel.getByRole('heading', { level: 2 })).toHaveText(name);
	await expect(page).toHaveURL(/\/graph\?id=/);
	await expect(drawing).toHaveAttribute('data-moving', 'false');
	// The owner stays a neighbour, named as you.
	await expect(page.getByTestId('graph-links').getByRole('button', { name: 'Centrer le graphe sur Vous' })).toBeVisible();

	// A click in the drawing recentres too.
	const trail = page.getByRole('navigation', { name: 'Fiches parcourues' });
	await expect(trail.getByRole('listitem')).toHaveText(['Vous', name]);
	const other = drawing.locator('.ego-node').filter({ hasNotText: 'Vous' }).first();
	await other.click();
	await expect(panel.getByRole('heading', { level: 2 })).not.toHaveText(name);
	await expect(trail.getByRole('listitem')).toHaveCount(3);

	// Back by the trail.
	await trail.getByRole('link', { name: 'Vous' }).click();
	await expect(panel.getByRole('heading', { level: 2 })).toHaveText('Vous');
	await expect(page.getByRole('navigation', { name: 'Fiches parcourues' })).toHaveCount(0);
});

test('filters by kind, and the period counted', async ({ page }) => {
	await page.goto(ipse.url);
	await page.goto(`${ipse.origin}/graph`);
	const links = page.getByTestId('graph-links').getByRole('listitem');
	await expect(links.first()).toBeVisible();
	const kinds = page.getByRole('group', { name: 'Montrer' });
	await kinds.getByRole('button', { name: 'Personnes' }).click();
	await expect(page).toHaveURL(/kinds=/);
	await expect(page.getByTestId('graph-panel')).toContainText('Certains liens sont masqués par les filtres.');
	await expect(links.filter({ hasText: 'Maman' })).toHaveCount(0);
	await kinds.getByRole('button', { name: 'Personnes' }).click();
	await expect(page).not.toHaveURL(/kinds=/);

	const before = Number((await links.first().textContent())?.match(/(\d+) événements/)?.[1]);
	await page.getByRole('button', { name: 'Depuis toujours' }).click();
	await expect(page).toHaveURL(/period=all/);
	await expect(links.first()).not.toContainText('sur 12 mois');
	const after = Number((await links.first().textContent())?.match(/(\d+) événements/)?.[1]);
	expect(after).toBeGreaterThanOrEqual(before);
});

test('a file’s page leads around it, then to its graph; old addresses lead to the graph', async ({ page }) => {
	await page.goto(ipse.url);
	await page.goto(`${ipse.origin}/memory/places`);
	await page.getByRole('link', { name: 'Cabinet dentaire du Parc' }).click();
	await page.getByRole('link', { name: 'Tout ce qui touche Cabinet dentaire du Parc' }).click();
	await expect(page).toHaveURL(/\/around\?id=/);
	await page.getByRole('link', { name: 'Voir Cabinet dentaire du Parc dans le graphe' }).click();
	await expect(page).toHaveURL(/\/graph\?id=/);
	const panel = page.getByTestId('graph-panel');
	await expect(panel.getByRole('heading', { level: 2 })).toHaveText('Cabinet dentaire du Parc');
	await expect(panel).toContainText('Pas visible par vos IA');
	const id = new URL(page.url()).searchParams.get('id');

	await page.goto(`${ipse.origin}/memory?graph=1&id=${id}`);
	await expect(page).toHaveURL(new RegExp(`/graph\\?id=${id}`));
	await expect(panel.getByRole('heading', { level: 2 })).toHaveText('Cabinet dentaire du Parc');
	// Mémoire is a list again.
	await page.getByRole('navigation', { name: 'Sections' }).getByRole('link', { name: 'Mémoire' }).click();
	await expect(page.getByTestId('kinds')).toBeVisible();
	await expect(page.getByRole('button', { name: 'Vue graphe' })).toHaveCount(0);
});

test('on a phone, the graph takes the whole width', async ({ page }) => {
	await page.setViewportSize({ width: 390, height: 844 });
	await page.goto(ipse.url);
	await page.goto(`${ipse.origin}/graph`);
	const drawing = page.getByTestId('graph');
	await expect(drawing).toBeVisible();
	expect((await drawing.boundingBox())!.width).toBeGreaterThan(330);
	expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
});
