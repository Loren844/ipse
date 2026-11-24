import { expect, test } from '@playwright/test';
import { startDemo, type Running } from './ipse.ts';

// The whole memory, not only people (issue #128), on the demo memory.
let ipse: Running;
test.beforeEach(async () => {
	ipse = await startDemo('fr');
});
test.afterEach(() => ipse.stop());

test('every kind of file, a place with its events, what links to an organization', async ({ page }) => {
	await page.goto(ipse.url);
	await page.getByRole('navigation', { name: 'Sections' }).getByRole('link', { name: 'Mémoire' }).click();
	await expect(page.getByRole('heading', { level: 1 })).toHaveText('Mémoire');
	const kinds = page.getByTestId('kinds');
	await expect(kinds.getByRole('link')).toHaveText([
		/Personnes\s*149/,
		/Lieux\s*10/,
		/Organisations\s*3/,
		/Projets\s*2/,
		/Œuvres\s*3/,
		/Objets\s*2/,
		/Sujets\s*2/,
		/Rappels\s*0/
	]);
	await expect(page.getByTestId('now')).toContainText('Les Jours de sel');

	await kinds.getByRole('link', { name: /Lieux/ }).click();
	await expect(page).toHaveURL(`${ipse.origin}/memory/places`);
	await expect(page.getByTestId('people').getByRole('link')).toHaveText([
		/Biarritz/,
		/Cabinet dentaire du Parc/,
		/Chez Capucine/,
		/Chez Jade/,
		/Chez Paul/,
		/Chez Rose/,
		/Domicile/,
		/Lyon/,
		/Studio Lotus/,
		/Travail/
	]);
	await page.getByRole('link', { name: 'Studio Lotus' }).click();
	await expect(page.getByRole('heading', { level: 1 })).toHaveText('Studio Lotus');
	await expect(page.getByText('Cours de yoga le mardi soir.')).toBeVisible();
	await expect(page.getByRole('heading', { level: 2, name: 'Événements' })).toBeVisible();
	await expect(page.locator('.thread li').first()).toContainText('Yoga');
	await expect(page.getByText('Vient d’une proposition acceptée et votre historique des positions.')).toBeVisible();

	await page.getByRole('link', { name: 'Mémoire', exact: true }).first().click();
	await kinds.getByRole('link', { name: /Organisations/ }).click();
	await page.getByRole('link', { name: 'Atelier Nord' }).click();
	const back = page.getByTestId('backlinks');
	await expect(back.getByRole('link', { name: /Camille Laurent/ })).toBeVisible();
	await expect(back).toContainText('travaille chez');
	await back.getByRole('link', { name: /Camille Laurent/ }).click();
	await expect(page.getByRole('heading', { level: 1 })).toHaveText('Camille Laurent');
	await expect(page.getByRole('link', { name: 'Atelier Nord' })).toBeVisible();
});

test('search the whole memory from any screen', async ({ page }) => {
	await page.goto(ipse.url);
	await expect(page.getByTestId('now')).toContainText('Mira Sol');
	await page.getByRole('searchbox', { name: 'Chercher dans la mémoire' }).fill('escalade');
	await page.keyboard.press('Enter');
	await expect(page).toHaveURL(`${ipse.origin}/memory?q=escalade`);
	const found = page.getByTestId('found');
	await expect(found.getByRole('link').first()).toContainText('Club d’escalade Vertige');
	await expect(found).toContainText('Sujet');
	await expect(found).not.toContainText('[[');
	await found.getByRole('link', { name: /^Escalade/ }).click();
	await expect(page).toHaveURL(/\/memory\/topics\//);
	await expect(page.getByRole('heading', { level: 1 })).toHaveText('Escalade');
});

test('the memory screen has its own search box, which a rail search fills', async ({ page }) => {
	await page.goto(ipse.url);
	await page.goto(`${ipse.origin}/memory`);
	const box = page.getByRole('main').getByRole('searchbox', { name: 'Chercher dans la mémoire' });
	const found = page.getByTestId('found');
	await box.pressSequentially('yoga', { delay: 40 });
	await expect(box).toHaveValue('yoga');
	await expect(found.getByRole('link').first()).toContainText('Yoga');
	await expect(page).toHaveURL(`${ipse.origin}/memory?q=yoga`);

	// A search from the rail lands here; the box can then be changed.
	await page.getByRole('banner').getByRole('searchbox', { name: 'Chercher dans la mémoire' }).fill('escalade');
	await page.keyboard.press('Enter');
	await expect(box).toHaveValue('escalade');
	await expect(found.getByRole('link').first()).toContainText('Club d’escalade Vertige');
	await box.fill('');
	await box.pressSequentially('lotus', { delay: 40 });
	await expect(box).toHaveValue('lotus');
	await expect(found.getByRole('link').first()).toContainText('Studio Lotus');
	await expect(page).toHaveURL(`${ipse.origin}/memory?q=lotus`);
});

test('a kind with nothing yet says how it arrives', async ({ page }) => {
	await page.goto(ipse.url);
	await page.goto(`${ipse.origin}/memory/things`);
	await expect(page.getByTestId('people').getByRole('link')).toHaveCount(2);
	await page.getByLabel('Chercher : objets').fill('zzz');
	await expect(page.getByText('Rien ne correspond.')).toBeVisible();
	await page.goto(`${ipse.origin}/memory/nowhere`);
	await expect(page.getByRole('heading', { level: 1 })).toHaveText('Cette fiche n’existe pas, ou plus.');
});
