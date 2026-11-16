import { expect, test, type Page } from '@playwright/test';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { execFileSync } from 'node:child_process';
import { IPSE, startIpse, type Running } from './ipse.ts';

// Proposals as an AI leaves them (docs/spec/vault.md §12), on the invented test contacts.
type Change = Record<string, unknown>;
function propose(vault: string, id: string, reason: string, changes: Change[], minutes = 30) {
	const created = new Date(Date.now() - minutes * 60_000).toISOString();
	const proposal = { format: 1, id, by: 'client:claude-desktop', created, reason, categories: ['people'], changes, status: { state: 'waiting' } };
	mkdirSync(join(vault, '.ipse/proposals'), { recursive: true });
	writeFileSync(join(vault, `.ipse/proposals/${id}.json`), JSON.stringify(proposal));
}

const MARC = 'people/marc-leroy.md';
const ELODIE = 'people/elodie-martin.md';

let ipse: Running;
test.beforeEach(async () => {
	ipse = await startIpse('Inès Roux', 'fr', true, (vault) => {
		execFileSync(IPSE, ['client', 'add', 'Claude Desktop', '--propose', 'people', '--vault', vault]);
		propose(vault, '01jd3k8m2p4q6r8t0v2w4y6z8a', 'Marc l’a dit pendant notre conversation.', [
			{ kind: 'add_fact', file: MARC, text: 'Joue au tennis le jeudi.' },
			{ kind: 'set_key', file: MARC, key: 'phones', from: ['+33123456789'], to: ['+33123456789', '+33612345678'] }
		], 50);
		propose(vault, '01jd3k9m2p4q6r8t0v2w4y6z8a', 'Vu dans un message d’Élodie.', [{ kind: 'add_fact', file: ELODIE, text: 'Parle japonais.' }], 40);
		propose(vault, '01jd3kam2p4q6r8t0v2w4y6z8a', 'Élodie en a parlé.', [{ kind: 'add_fact', file: ELODIE, text: 'Aime le jazz.' }], 30);
		propose(vault, '01jd3kbm2p4q6r8t0v2w4y6z8a', 'Julie était au dîner.', [
			{ kind: 'create', file: 'people/julie-dupont.md', type: 'person', keys: { name: 'Julie Dupont' }, facts: ['Amie de Marc.'] }
		], 20);
		// Written for an email Marc no longer has: it no longer applies.
		propose(vault, '01jd3kcm2p4q6r8t0v2w4y6z8a', 'Marc a changé d’adresse.', [
			{ kind: 'set_key', file: MARC, key: 'emails', from: ['ancien@example.org'], to: ['marc@example.net'] }
		], 10);
	});
});
test.afterEach(() => ipse.stop());

const read = (file: string) => readFileSync(join(ipse.vault, file), 'utf8');
const count = (page: Page) => page.getByRole('link', { name: /À revoir/ }).locator('.count');

test('review proposals: accept, refuse with a comment, accept several, undo', async ({ page }) => {
	await page.goto(ipse.url);
	await expect(count(page)).toHaveText('5');
	await page.getByRole('link', { name: /À revoir/ }).click();
	await expect(page.getByRole('heading', { level: 1 })).toHaveText('À revoir');
	await expect(page.getByText('5 propositions attendent votre décision.')).toBeVisible();
	// Newest first, with who, why and what it touches.
	const lines = page.getByTestId('proposal');
	await expect(lines.locator('.reason')).toHaveText([
		'Marc a changé d’adresse.',
		'Julie était au dîner.',
		'Élodie en a parlé.',
		'Vu dans un message d’Élodie.',
		'Marc l’a dit pendant notre conversation.'
	]);
	await expect(lines.first()).toContainText('Proposée par Claude Desktop');
	await expect(lines.first()).toContainText('Ne s’applique plus');
	await expect(lines.nth(1)).toContainText('Julie Dupont');

	// One proposal in plain words: added in green with +, never a fact id or front matter.
	await page.getByRole('link', { name: 'Marc l’a dit pendant notre conversation.' }).click();
	await expect(page.getByRole('heading', { level: 1 })).toHaveText('Marc l’a dit pendant notre conversation.');
	await expect(page.getByText(/^Proposée par Claude Desktop, il y a 50 minutes$/)).toBeVisible();
	await expect(page).toHaveTitle('Marc l’a dit pendant notre conversation. · Ipse');
	await expect(page.getByRole('heading', { level: 2, name: 'Marc Leroy' })).toBeVisible();
	const added = page.locator('.diff .added');
	await expect(added).toHaveText([/Joue au tennis le jeudi\./, /\+33 6 12 34 56 78/]);
	await expect(page.locator('.diff .removed')).toHaveCount(0);
	await expect(page.getByText(/\^f-|phones:/)).toHaveCount(0);
	await page.getByRole('button', { name: 'Accepter' }).click();
	await expect(page.getByRole('status')).toContainText('Proposition acceptée.');
	await expect(count(page)).toHaveText('4');
	expect(read(MARC)).toContain('Joue au tennis le jeudi.');
	expect(read(MARC)).toContain('+33612345678');

	// Refuse, saying why: the AI reads it.
	await page.getByRole('link', { name: 'Revoir la suivante' }).click();
	await page.goto(`${ipse.origin}/review/01jd3k9m2p4q6r8t0v2w4y6z8a`);
	await page.getByRole('button', { name: 'Refuser' }).click();
	await expect(page.getByLabel('Pourquoi ? (facultatif)')).toBeFocused();
	await page.keyboard.type('Elle parle coréen, pas japonais.');
	await page.getByRole('button', { name: 'Refuser la proposition' }).click();
	await expect(page.getByRole('status')).toContainText('Proposition refusée.');
	await expect(page.getByText('Vous avez répondu : « Elle parle coréen, pas japonais. »')).toBeVisible();
	expect(read(ELODIE)).not.toContain('japonais');
	const saved = JSON.parse(read('.ipse/proposals/01jd3k9m2p4q6r8t0v2w4y6z8a.json'));
	expect(saved.status).toMatchObject({ state: 'refused', comment: 'Elle parle coréen, pas japonais.' });

	// A stale proposal says why and can only be refused.
	await page.goto(`${ipse.origin}/review/01jd3kcm2p4q6r8t0v2w4y6z8a`);
	await expect(page.getByText('Cette proposition ne s’applique plus.')).toBeVisible();
	await expect(page.getByRole('button', { name: 'Refuser' })).toBeVisible();
	await expect(page.getByRole('button', { name: 'Accepter' })).toHaveCount(0);
	await expect(page.locator('.diff .added')).toHaveText([/marc@example\.net/]);
	await expect(page.locator('.diff .removed')).toHaveText([/ancien@example\.org/]);

	// Several in one action, from the keyboard; each is still its own proposal.
	await page.getByRole('navigation').getByRole('link', { name: /À revoir/ }).click();
	await expect(lines).toHaveCount(3);
	await page.getByLabel('Sélectionner : Julie était au dîner.').check();
	await page.getByLabel('Sélectionner : Élodie en a parlé.').focus();
	await page.keyboard.press('Space');
	await page.getByRole('button', { name: 'Accepter les 2 propositions sélectionnées' }).click();
	await expect(page.getByRole('status')).toContainText('2 propositions acceptées.');
	await expect(lines).toHaveCount(1);
	await expect(count(page)).toHaveText('1');
	expect(read(ELODIE)).toContain('Aime le jazz.');
	expect(read('people/julie-dupont.md')).toContain('Amie de Marc.');
	const decided = page.getByTestId('decided');
	await expect(decided).toHaveCount(4);
	await expect(decided.filter({ hasText: 'Julie était au dîner.' })).toContainText('Acceptée');
	await expect(decided.filter({ hasText: 'Vu dans un message d’Élodie.' })).toContainText('Refusée');

	// Undo the first one accepted: Marc's file is back as it was.
	await decided.getByRole('link', { name: 'Marc l’a dit pendant notre conversation.' }).click();
	await page.getByRole('button', { name: 'Annuler' }).click();
	await expect(page.getByRole('status')).toContainText('Proposition annulée');
	await expect(page.locator('.badge')).toHaveText('Annulée');
	expect(read(MARC)).not.toContain('tennis');
	expect(read(MARC)).not.toContain('+33612345678');
});

test('nothing to review', async ({ page }) => {
	const empty = await startIpse('Inès Roux', 'fr');
	try {
		await page.goto(empty.url);
		await expect(page.getByTestId('memory-name')).toBeVisible();
		await page.getByRole('link', { name: /À revoir/ }).click();
		await expect(page.getByText('Rien à revoir pour le moment.')).toBeVisible();
		await page.goto(`${empty.origin}/review/01zzzzzzzzzzzzzzzzzzzzzzzz`);
		await expect(page.getByRole('heading', { level: 1 })).toHaveText('Cette proposition n’existe pas, ou plus.');
	} finally {
		empty.stop();
	}
});

test('a summary the AI wrote, accepted from the inbox', async ({ page }) => {
	const summary = await startIpse('Inès Roux', 'fr', true, (vault) => {
		propose(vault, '01jd3kdm2p4q6r8t0v2w4y6z8a', 'Résumé de samedi.', [
			{ kind: 'write_summary', file: 'summaries/day/2026/2026-11-14.md', period: 'day', start: '2026-11-14', text: 'Dîner chez Julie avec Marc.' }
		]);
	});
	try {
		await page.goto(summary.url);
		await page.getByRole('navigation', { name: 'Sections' }).getByRole('link', { name: /À revoir/ }).click();
		await expect(page.getByTestId('proposal')).toContainText('Samedi 14 novembre 2026');
		await page.getByRole('link', { name: 'Résumé de samedi.' }).click();
		await expect(page.getByRole('heading', { level: 2 })).toHaveText(/Résumé : Samedi 14 novembre 2026/);
		await expect(page.locator('.diff .what')).toHaveText(['Résumé']);
		await expect(page.locator('.diff .added')).toHaveText([/Dîner chez Julie avec Marc\./]);
		await page.getByRole('button', { name: 'Accepter' }).click();
		await expect(page.getByRole('status')).toContainText('Proposition acceptée.');
		const text = readFileSync(join(summary.vault, 'summaries/day/2026/2026-11-14.md'), 'utf8');
		expect(text).toContain('\n## Résumé\n\nDîner chez Julie avec Marc.\n');
		expect(text).toContain('\n## Événements\n\n- [[2026-11-14-diner-chez-julie]]');
	} finally {
		summary.stop();
	}
});
