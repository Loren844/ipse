import { expect, test } from '@playwright/test';
import { startDemo, type Running } from './ipse.ts';

// Kinds of events (issue #131), on the demo memory: the AI's proposal reviewed and
// accepted, then a kind set by hand.
let ipse: Running;
test.beforeEach(async () => {
	ipse = await startDemo('fr');
});
test.afterEach(() => ipse.stop());

test('accept the kinds the AI proposed, then set one by hand', async ({ page }) => {
	await page.goto(ipse.url);
	await page.getByRole('navigation', { name: 'Sections' }).getByRole('link', { name: /À revoir/ }).click();
	// Many changes, said in one line: how many events get which kind.
	const kinds = page.getByTestId('proposal').filter({ hasText: 'D’après le titre, le lieu et les personnes de chaque événement.' });
	await expect(kinds.locator('.title')).toHaveText(/^\d+ événements reçoivent un type : .*Repas \(\d+\)/);
	await kinds.locator('a.title').click();
	const first = page.locator('.diff li').first();
	await expect(first.locator('.what')).toHaveText('Type d’événement');
	await expect(page.locator('.diff').filter({ hasText: 'Repas' }).first()).toBeVisible();
	await expect(page.locator('.diff').filter({ hasText: 'meal' })).toHaveCount(0);
	await page.getByRole('button', { name: 'Accepter' }).click();
	await expect(page.getByRole('status')).toContainText('Proposition acceptée.');

	await page.getByRole('navigation', { name: 'Sections' }).getByRole('link', { name: 'Agenda' }).click();
	await expect(page.locator('.knot', { hasText: 'Déjeuner avec' }).locator('.kind-tag', { hasText: 'Repas' }).first()).toBeVisible();

	// An earlier event the AI was not asked about yet: the person sets its kind.
	await page.getByRole('button', { name: 'Plus tôt' }).click();
	const bare = page.locator('.knot:not(:has(.kind-tag))').first();
	await bare.locator('a.title').click();
	await page.getByRole('button', { name: 'Modifier' }).click();
	await expect(page.getByLabel('Type d’événement')).toHaveValue('');
	await page.getByLabel('Type d’événement').selectOption({ label: 'Sport' });
	await page.getByRole('button', { name: 'Enregistrer' }).click();
	await expect(page.getByText('Enregistré. Vous pouvez revenir en arrière depuis l’historique.')).toBeVisible();
	await expect(page.locator('.page-head .kind-tag')).toHaveText('Sport');
});
