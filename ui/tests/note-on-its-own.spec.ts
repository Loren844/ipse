import { execFileSync } from 'node:child_process';
import { expect, test } from '@playwright/test';
import { IPSE, startIpse, type Running } from './ipse.ts';
import { connectAi } from './mcp.ts';

// An AI that notes on its own what is worth remembering (issue #246): a switch per AI,
// shown only once it proposes somewhere, and a mark on what it noted in the inbox.
const cases = [
	{
		language: 'fr',
		scheme: 'light' as const,
		ais: 'IA connectées',
		inbox: /À revoir/,
		title: 'Retenir d’elle-même',
		toggle: 'Laisser Claude retenir d’elle-même ce qui compte',
		on: 'Claude proposera d’elle-même ce qui mérite d’être retenu.',
		off: 'Claude ne proposera plus que ce que vous lui demandez.',
		mark: 'Notée d’elle-même'
	},
	{
		language: 'en',
		scheme: 'dark' as const,
		ais: 'Connected AIs',
		inbox: /To review/,
		title: 'Note on its own',
		toggle: 'Let Claude note on its own what matters',
		on: 'Claude will propose on its own what is worth remembering.',
		off: 'Claude will only propose what you ask for.',
		mark: 'Noted on its own'
	}
];

for (const t of cases) {
	test.describe(`${t.language}, ${t.scheme}`, () => {
		let ipse: Running;
		let token = '';
		test.beforeEach(async () => {
			ipse = await startIpse('Inès Roux', t.language, false, (vault) => {
				const printed = execFileSync(IPSE, ['client', 'add', 'Claude', '--propose', 'things', '--vault', vault]).toString();
				token = printed.split(/\s+/).find((w) => w.startsWith('ipse_'))!;
				execFileSync(IPSE, ['client', 'add', 'Autre', '--vault', vault]);
			});
		});
		test.afterEach(() => ipse.stop());

		test('the switch, then what the AI noted on its own is marked', async ({ page }) => {
			await page.emulateMedia({ colorScheme: t.scheme });
			await page.goto(ipse.url);
			const sections = page.getByRole('navigation', { name: /Sections/ });
			await sections.getByRole('link', { name: t.ais }).click();

			// Only for an AI that proposes somewhere.
			const other = page.getByTestId('ai').filter({ hasText: 'Autre' });
			await expect(other.getByRole('heading', { name: t.title })).toHaveCount(0);
			const claude = page.getByTestId('ai').filter({ hasText: 'Claude' });
			await expect(claude.getByRole('heading', { name: t.title })).toBeVisible();
			const toggle = claude.getByRole('switch', { name: t.toggle });
			await expect(toggle).not.toBeChecked();
			await toggle.click();
			await expect(page.getByRole('status')).toContainText(t.on);
			await expect(toggle).toBeChecked();

			// The AI's side: one fact noted on its own, one asked for.
			const ai = await connectAi(ipse.vault, token);
			await ai.call('propose', {
				on_its_own: true,
				reason: 'Dit en passant : « mon vélo est un Peugeot de 1982 ».',
				changes: [{ kind: 'create', type: 'thing', keys: { name: 'Vélo Peugeot' }, facts: ['Date de 1982.'] }]
			});
			await ai.call('propose', {
				reason: 'Inès a demandé de retenir sa console.',
				changes: [{ kind: 'create', type: 'thing', keys: { name: 'Console' } }]
			});
			ai.close();

			await sections.getByRole('link', { name: t.inbox }).click();
			const proposals = page.getByTestId('proposal');
			await expect(proposals).toHaveCount(2);
			const noted = proposals.filter({ hasText: 'Peugeot' });
			await expect(noted.getByTestId('on-its-own')).toHaveText(t.mark);
			await expect(proposals.filter({ hasText: 'console' }).getByTestId('on-its-own')).toHaveCount(0);
			await noted.getByRole('link').first().click();
			await expect(page.getByTestId('on-its-own')).toHaveText(t.mark);

			// Turned off again.
			await sections.getByRole('link', { name: t.ais }).click();
			await page.getByTestId('ai').filter({ hasText: 'Claude' }).getByRole('switch', { name: t.toggle }).click();
			await expect(page.getByRole('status')).toContainText(t.off);
			await expect(page.getByTestId('ai').filter({ hasText: 'Claude' }).getByRole('switch', { name: t.toggle })).not.toBeChecked();
		});
	});
}
