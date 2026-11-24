import { expect, test } from '@playwright/test';
import { startDemo, type Running } from './ipse.ts';

// The graph of the whole memory (issue #174), in Mémoire, on the demo memory.
let ipse: Running;
test.beforeEach(async () => {
	ipse = await startDemo('fr');
});
test.afterEach(() => ipse.stop());

test('the graph opens beside the list, on the owner, and follows the list', async ({ page }) => {
	await page.goto(ipse.url);
	// No section of its own any more.
	await expect(page.getByRole('navigation', { name: 'Sections' }).getByRole('link', { name: 'Graphe' })).toHaveCount(0);
	await page.getByRole('navigation', { name: 'Sections' }).getByRole('link', { name: 'Mémoire' }).click();
	await page.getByRole('button', { name: 'Vue graphe' }).click();
	await expect(page).toHaveURL(/\/memory\?graph=1/);
	await expect(page.getByRole('button', { name: 'Vue graphe' })).toHaveAttribute('aria-pressed', 'true');

	const drawing = page.getByRole('application', { name: /Graphe de votre mémoire : \d+ fiches, \d+ liens/ });
	await expect(drawing).toBeVisible();
	await expect(page.getByTestId('memory-graph')).toHaveAttribute('data-files', /^\d{3}$/);
	// The kinds, each with its shape, and what AIs cannot see, in the legend.
	const legend = page.getByRole('list', { name: 'Légende' });
	await expect(legend.getByRole('listitem')).toHaveText(['Vous', 'Personnes', 'Lieux', 'Organisations', 'Projets', 'Œuvres', 'Objets', 'Sujets', 'Événements', 'Pas visible par vos IA']);

	// The owner is chosen first: their links, as a list for the keyboard.
	const panel = page.getByTestId('graph-panel');
	await expect(panel.getByRole('heading', { level: 2 })).toHaveText('Camille Laurent');
	await expect(panel.getByRole('heading', { level: 3, name: /\d+ liens/ })).toBeVisible();
	await panel.getByRole('button', { name: 'Choisir Élise Laurent' }).click();
	await expect(panel.getByRole('heading', { level: 2 })).toHaveText('Élise Laurent');
	await expect(page).toHaveURL(/id=/);
	await expect(panel.getByRole('link', { name: 'Ouvrir sa fiche' })).toBeVisible();

	// A file chosen in the list is chosen in the graph, not opened.
	await page.locator('#search').fill('Atelier Nord');
	await page.getByTestId('found').getByRole('link', { name: 'Atelier Nord Organisation' }).click();
	await expect(page).toHaveURL(/\/memory\?/);
	await expect(panel.getByRole('heading', { level: 2 })).toHaveText('Atelier Nord');
	await expect(page.getByTestId('found').getByRole('link', { name: 'Atelier Nord Organisation' })).toHaveAttribute('aria-current', 'true');

	// Filters: people hidden, the organization's links to them are left out.
	await page.getByText('Filtres').click();
	await page.getByRole('group', { name: 'Montrer' }).getByRole('button', { name: 'Personnes' }).click();
	await expect(page).toHaveURL(/kinds=/);
	await expect(panel).toContainText('Ses liens vers des fiches masquées par les filtres ne sont pas montrés.');
	await page.getByRole('group', { name: 'Montrer' }).getByRole('button', { name: 'Personnes' }).click();
	await expect(page).not.toHaveURL(/kinds=/);

	// The keyboard moves and zooms the drawing.
	const frames = Number(await page.getByTestId('memory-graph').getAttribute('data-frames'));
	await drawing.focus();
	await page.keyboard.press('+');
	await page.keyboard.press('ArrowLeft');
	await expect.poll(async () => Number(await page.getByTestId('memory-graph').getAttribute('data-frames'))).toBeGreaterThan(frames);

	// Closing it gives the list its room back.
	await page.getByRole('button', { name: 'Vue graphe' }).click();
	await expect(drawing).toHaveCount(0);
	await expect(page.getByTestId('kinds')).toBeVisible();
});

test('a file opens in the graph, and a sensitive category is marked', async ({ page }) => {
	await page.goto(ipse.url);
	await page.goto(`${ipse.origin}/memory/places`);
	await page.getByRole('link', { name: 'Cabinet dentaire du Parc' }).click();
	await page.getByRole('link', { name: 'Voir Cabinet dentaire du Parc dans le graphe' }).click();
	await expect(page).toHaveURL(/\/memory\?graph=1&id=/);
	const panel = page.getByTestId('graph-panel');
	await expect(panel.getByRole('heading', { level: 2 })).toHaveText('Cabinet dentaire du Parc');
	await expect(panel).toContainText('Pas visible par vos IA');
	await page.getByText('Filtres').click();
	const health = page.getByRole('group', { name: 'Catégories' }).getByRole('button', { name: /santé/ });
	await expect(health.getByRole('img', { name: 'Pas visible par vos IA' })).toBeVisible();
	await health.click();
	await expect(health).toHaveAttribute('aria-pressed', 'false');
	await expect(page).toHaveURL(/hide=health/);

	// An old address of the graph leads to the same place.
	const id = new URL(page.url()).searchParams.get('id');
	await page.goto(`${ipse.origin}/graph?id=${id}`);
	await expect(page).toHaveURL(new RegExp(`/memory\\?graph=1&id=${id}`));
	await expect(panel.getByRole('heading', { level: 2 })).toHaveText('Cabinet dentaire du Parc');
});

test('on a phone, the graph takes the whole width', async ({ page }) => {
	await page.setViewportSize({ width: 390, height: 844 });
	await page.goto(ipse.url);
	await page.goto(`${ipse.origin}/memory?graph=1`);
	await expect(page.getByTestId('memory-graph')).toBeVisible();
	await expect(page.getByTestId('kinds')).toBeHidden();
	const box = (await page.getByTestId('memory-graph').boundingBox())!;
	expect(box.width).toBeGreaterThan(330);
	const scroll = await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth);
	expect(scroll).toBe(true);
});

test('in the app’s window, the graph opens whole, with you inside the window (#198)', async ({ page }) => {
	await page.setViewportSize({ width: 1200, height: 800 });
	await page.goto(ipse.url);
	await page.goto(`${ipse.origin}/memory?graph=1`);
	const canvas = page.getByTestId('memory-graph');
	await expect(canvas).toHaveAttribute('data-owner', /^-?\d+,-?\d+$/);
	const [x, y] = (await canvas.getAttribute('data-owner'))!.split(',').map(Number);
	const box = (await canvas.boundingBox())!;
	// The frame goes below the window before the page scrolls; you are not below it.
	expect(box.y + box.height).toBeGreaterThan(800);
	expect(box.x + x!).toBeGreaterThan(box.x);
	expect(box.x + x!).toBeLessThan(box.x + box.width);
	expect(box.y + y!).toBeGreaterThan(box.y);
	expect(box.y + y!).toBeLessThan(800);
	// The whole memory, top to bottom, is in the window.
	const [top, bottom] = (await canvas.getAttribute('data-extent'))!.split(',').map(Number);
	expect(box.y + top!).toBeGreaterThanOrEqual(box.y);
	expect(box.y + bottom!).toBeLessThanOrEqual(800);
	// And it is centred in what the window shows of the frame, not in the whole frame.
	expect(Math.abs(box.y + (top! + bottom!) / 2 - (box.y + 800) / 2)).toBeLessThan(20);
});
