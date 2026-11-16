import { expect, test } from '@playwright/test';
import { execFileSync } from 'node:child_process';
import { existsSync, readFileSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { IPSE, startIpse, type Running } from './ipse.ts';
import { connectAi } from './mcp.ts';

// Milestone 2 (issue #84), the person's side: an AI goes through its tasks over MCP and
// proposes; the person accepts, refuses with a comment and undoes in the browser.
// `crates/ipse/tests/milestone2.rs` runs the same scenario on every system.
const CONTACTS = resolve(import.meta.dirname, '../../crates/ipse/tests/fixtures/contacts/milestone2.vcf');

/** Yesterday in Paris, `YYYY-MM-DD`. */
function yesterday(): string {
	const now = new Date(Date.now() - 86_400_000);
	return new Intl.DateTimeFormat('en-CA', { timeZone: 'Europe/Paris' }).format(now);
}

let ipse: Running;
let token = '';
test.beforeEach(async () => {
	ipse = await startIpse('Inès Roux', 'fr', false, (vault) => {
		execFileSync(IPSE, ['import', CONTACTS, '--vault', vault]);
		const day = yesterday().replaceAll('-', '');
		const agenda = join(vault, '..', 'agenda.ics');
		writeFileSync(
			agenda,
			`BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Ipse tests//EN\r\nBEGIN:VEVENT\r\nUID:dinner-milestone2@example.org\r\nDTSTAMP:${day}T080000Z\r\nDTSTART:${day}T183000Z\r\nDTEND:${day}T203000Z\r\nSUMMARY:Dîner avec Élodie\r\nATTENDEE;CN=Élodie Petit:mailto:elodie.petit@example.org\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n`
		);
		execFileSync(IPSE, ['import', agenda, '--vault', vault]);
		const printed = execFileSync(IPSE, ['client', 'add', 'Claude', '--propose', 'people,agenda,summaries', '--vault', vault]).toString();
		token = printed.split(/\s+/).find((w) => w.startsWith('ipse_'))!;
	});
});
test.afterEach(() => ipse.stop());

const read = (file: string) => readFileSync(join(ipse.vault, file), 'utf8');

test('the AI tidies up, the person decides', async ({ page }) => {
	// "Tidy up my memory": the AI's side, over MCP.
	const ai = await connectAi(ipse.vault, token);
	const { tasks } = await ai.call('tasks');
	const id = (kind: string) => tasks.find((t: { kind: string }) => t.kind === kind).id as string;
	const day = yesterday();
	await ai.call('propose', {
		task: id('extract'),
		reason: 'Lu dans les notes d’Élodie.',
		changes: [{ kind: 'add_fact', file: 'people/elodie-petit.md', text: 'Infirmière à Nantes.' }]
	});
	await ai.call('propose', {
		reason: 'Ses notes disent que Bob, c’est Robert.',
		changes: [{ kind: 'merge', keep: 'people/robert-martin.md', remove: 'people/bob-martin.md', clues: ['name'] }]
	});
	await ai.call('propose', {
		task: id('question'),
		reason: 'Inès m’a dit que c’est Jo Lambert, son plombier.',
		changes: [{ kind: 'set_key', file: 'people/jo-plombier.md', key: 'name', from: 'Jo plombier', to: 'Jo Lambert' }]
	});
	// A task answered waits for the person: the AI does not see it as open.
	expect((await ai.call('tasks', { kind: 'extract' })).tasks).toEqual([]);
	await ai.call('propose', {
		task: id('summary'),
		reason: 'Résumé d’hier.',
		changes: [{ kind: 'write_summary', period: 'day', start: day, text: 'Dîner avec Élodie.' }]
	});

	// The person's side, in the browser.
	await page.goto(ipse.url);
	const inbox = page.getByRole('navigation', { name: 'Sections' }).getByRole('link', { name: /À revoir/ });
	await expect(inbox.locator('.count')).toHaveText('5');
	await inbox.click();
	const lines = page.getByTestId('proposal');
	await expect(lines).toHaveCount(5);
	await expect(page.getByTestId('question')).toHaveCount(0);

	// Three accepted at once.
	for (const reason of ['Résumé d’hier.', 'Inès m’a dit que c’est Jo Lambert, son plombier.', 'Ses notes disent que Bob, c’est Robert.'])
		await page.getByLabel(`Sélectionner : ${reason}`).check();
	await page.getByRole('button', { name: 'Accepter les 3 propositions sélectionnées' }).click();
	await expect(page.getByRole('status').first()).toContainText('3 propositions acceptées.');
	expect(read('people/jo-plombier.md')).toContain('name: Jo Lambert\n');
	expect(read(`summaries/day/${day.slice(0, 4)}/${day}.md`)).toContain('Dîner avec Élodie.');
	expect(existsSync(join(ipse.vault, 'people/bob-martin.md'))).toBe(false);

	// Ipse's duplicate, merged from its page.
	await page.getByRole('link', { name: /Même adresse email/ }).click();
	await page.getByRole('button', { name: 'Fusionner' }).click();
	await expect(page.getByRole('status')).toContainText('Fusionnées');

	// The facts refused, saying why.
	await inbox.click();
	await page.getByRole('link', { name: 'Lu dans les notes d’Élodie.' }).click();
	await page.getByRole('button', { name: 'Refuser' }).click();
	await page.keyboard.type('Sage-femme, pas infirmière.');
	await page.getByRole('button', { name: 'Refuser la proposition' }).click();
	await expect(page.getByRole('status')).toContainText('Proposition refusée.');
	expect(read('people/elodie-petit.md')).not.toContain('## Faits');

	// The merge of Bob undone: his file is back.
	await inbox.click();
	await expect(page.getByText('Rien à revoir pour le moment.')).toBeVisible();
	await page.getByTestId('decided').getByRole('link', { name: 'Ses notes disent que Bob, c’est Robert.' }).click();
	await page.getByRole('button', { name: 'Annuler' }).click();
	await expect(page.getByRole('status')).toContainText('Proposition annulée');
	expect(existsSync(join(ipse.vault, 'people/bob-martin.md'))).toBe(true);

	// The AI reads the refusal, and its task is open again with the comment.
	const again = (await ai.call('tasks', { kind: 'extract' })).tasks;
	expect(again).toHaveLength(1);
	expect(again[0].comment).toBe('Sage-femme, pas infirmière.');
	ai.close();
});
