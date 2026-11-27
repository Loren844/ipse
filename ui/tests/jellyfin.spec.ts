// Jellyfin on the Sources screen (issue #227): connect it, its only user followed at
// once, what was watched in the memory, disconnect it. A small server on this computer
// answers as Jellyfin's REST API. Data invented.
import { expect, test } from '@playwright/test';
import { createServer, type Server } from 'node:http';
import { mkdtempSync, readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { startIpse, type Running } from './ipse.ts';

const TOKEN = 'jf-key-9a3c51';
const daysAgo = (days: number) => new Date(Math.max(Date.now() - days * 86_400_000, new Date(new Date().getFullYear(), 0, 1, 12).getTime())).toISOString();

const USERS = [{ Id: 'u1', Name: 'Inès' }];
const ITEMS = {
	Items: [
		{ Id: 'e1', Type: 'Episode', Name: 'Le bureau', SeriesName: 'Les Archives', ParentIndexNumber: 1, IndexNumber: 1, UserData: { LastPlayedDate: daysAgo(1), Played: true } },
		{ Id: 'm1', Type: 'Movie', Name: 'Le Phare', ProductionYear: 2019, UserData: { LastPlayedDate: daysAgo(2), Played: true } }
	],
	TotalRecordCount: 2
};

let ipse: Running;
let server: Server;
let secrets: string;
let address = '';

test.beforeEach(async () => {
	// A test machine has no keychain: a development build keeps the token in this file.
	secrets = join(mkdtempSync(join(tmpdir(), 'ipse-secrets-')), 'secrets.json');
	process.env.IPSE_TEST_SECRETS = secrets;
	server = createServer((request, response) => {
		if (!request.headers.authorization?.includes(`Token="${TOKEN}"`)) {
			response.writeHead(401);
			response.end();
			return;
		}
		const url = request.url ?? '';
		const body =
			url === '/Users'
				? USERS
				: url.startsWith('/Users/u1/Items?')
					? url.includes('Filters=IsPlayed')
						? ITEMS
						: { Items: [], TotalRecordCount: 0 }
					: null;
		response.writeHead(body === null ? 404 : 200, { 'content-type': 'application/json' });
		response.end(JSON.stringify(body ?? {}));
	});
	await new Promise<void>((done) => server.listen(0, '127.0.0.1', done));
	const port = server.address();
	address = `http://127.0.0.1:${typeof port === 'object' && port ? port.port : 0}`;
	ipse = await startIpse('Inès Roux', 'fr');
});
test.afterEach(() => {
	ipse.stop();
	server.close();
	delete process.env.IPSE_TEST_SECRETS;
});

test('connect Jellyfin, follow its only user, then disconnect it', async ({ page }) => {
	await page.goto(ipse.url);
	await page.getByRole('link', { name: 'Sources' }).first().click();
	await page.getByRole('link', { name: 'Jellyfin' }).click();
	const form = page.getByRole('region', { name: 'Connecter Jellyfin' });

	await form.getByLabel('Nom de la source').fill('Films');
	await form.getByLabel('Adresse de Jellyfin').fill(address);
	await form.getByLabel('Clé d’API').fill('not-the-key');
	await form.getByRole('button', { name: 'Connecter Jellyfin' }).click();
	await expect(page.getByRole('status')).toContainText('Jellyfin a refusé la clé d’API');

	await form.getByLabel('Clé d’API').fill(TOKEN);
	await form.getByRole('button', { name: 'Connecter Jellyfin' }).click();
	// Its only user is followed at once, and what they watched is read.
	await expect(page.getByRole('status')).toHaveText('Synchronisé.');
	await expect(form).toHaveCount(0);
	expect(readFileSync(secrets, 'utf8')).toContain(TOKEN);
	expect(JSON.stringify(await page.evaluate(() => fetch('/api/sources').then((r) => r.json())))).not.toContain(TOKEN);
	const source = page.getByTestId('sources').getByRole('listitem').filter({ hasText: 'Films' });
	await expect(source).toContainText('Suit Inès.');
	await expect(source).toContainText('la clé circule sans chiffrement');

	await source.getByRole('button', { name: 'Ne plus suivre' }).click();
	const dialog = page.getByRole('dialog', { name: 'Ne plus suivre « Films » ?' });
	await expect(dialog).toContainText('oubliera sa clé');
	await dialog.getByRole('button', { name: 'Ne plus suivre' }).click();
	await expect(page.getByRole('status')).toContainText('« Films » n’est plus suivi.');
	expect(readFileSync(secrets, 'utf8')).not.toContain(TOKEN);
});
