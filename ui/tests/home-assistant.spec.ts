// Home Assistant on the Sources screen (issue #225): connect it, choose whom to follow,
// see the stays at home, disconnect it. A small server on this computer answers as Home
// Assistant's REST API. Data invented.
import { expect, test } from '@playwright/test';
import { createServer, type Server } from 'node:http';
import { mkdtempSync, readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { startIpse, type Running } from './ipse.ts';

const TOKEN = 'ha-token-4c8e1a';
const hoursAgo = (hours: number) => new Date(Date.now() - hours * 3_600_000).toISOString();

const STATES = [
	{ entity_id: 'person.ines', state: 'home', attributes: { friendly_name: 'Inès' } },
	{ entity_id: 'person.paul', state: 'not_home', attributes: { friendly_name: 'Paul' } },
	{ entity_id: 'light.salon', state: 'on', attributes: {} }
];
const HISTORY = [
	[
		{ entity_id: 'person.ines', state: 'home', last_changed: hoursAgo(30) },
		{ state: 'not_home', last_changed: hoursAgo(20) },
		{ state: 'Atelier', last_changed: hoursAgo(19) },
		{ state: 'home', last_changed: hoursAgo(2) }
	]
];

let ipse: Running;
let server: Server;
let secrets: string;
let address = '';

test.beforeEach(async () => {
	// A test machine has no keychain: a development build keeps the token in this file.
	secrets = join(mkdtempSync(join(tmpdir(), 'ipse-secrets-')), 'secrets.json');
	process.env.IPSE_TEST_SECRETS = secrets;
	server = createServer((request, response) => {
		if (request.headers.authorization !== `Bearer ${TOKEN}`) {
			response.writeHead(401, { 'content-type': 'application/json' });
			response.end('{"message":"Unauthorized"}');
			return;
		}
		const url = request.url ?? '';
		const body = url === '/api/states' ? STATES : url.startsWith('/api/history/period/') ? HISTORY : null;
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

test('connect Home Assistant, choose whom to follow, then disconnect it', async ({ page }) => {
	await page.goto(ipse.url);
	await page.getByRole('link', { name: 'Sources' }).first().click();
	const form = page.getByRole('region', { name: 'Connecter Home Assistant' });

	await form.getByLabel('Nom de la source').fill('Maison');
	await form.getByLabel('Adresse de Home Assistant').fill(address);
	await form.getByLabel('Jeton d’accès longue durée').fill('not-the-token');
	await form.getByRole('button', { name: 'Connecter Home Assistant' }).click();
	await expect(page.getByRole('status')).toContainText('Home Assistant a refusé le jeton d’accès');

	await form.getByLabel('Jeton d’accès longue durée').fill(TOKEN);
	await form.getByRole('button', { name: 'Connecter Home Assistant' }).click();
	await expect(page.getByRole('status')).toHaveText('« Maison » est connecté. Choisissez qui suivre.');
	// The token leaves the page, and is kept only in the keychain (here, the test file).
	await expect(form.getByLabel('Jeton d’accès longue durée')).toHaveValue('');
	expect(readFileSync(secrets, 'utf8')).toContain(TOKEN);
	expect(JSON.stringify(await page.evaluate(() => fetch('/api/sources').then((r) => r.json())))).not.toContain(TOKEN);

	// Nobody's presence is read before the person chooses; http:// is said.
	const source = page.getByTestId('sources').getByRole('listitem').filter({ hasText: 'Maison' });
	await expect(source).toContainText('le jeton circule sans chiffrement');
	await expect(source.getByRole('heading', { name: 'Choisissez qui suivre' })).toBeVisible();
	await source.getByLabel('Inès').check();
	await source.getByRole('button', { name: 'Suivre cette personne' }).click();
	await expect(page.getByRole('status')).toHaveText('Synchronisé.');
	await expect(source).toContainText('Suit Inès.');

	await source.getByRole('button', { name: 'Ne plus suivre' }).click();
	const dialog = page.getByRole('dialog', { name: 'Ne plus suivre « Maison » ?' });
	await expect(dialog).toContainText('oubliera son jeton');
	await dialog.getByRole('button', { name: 'Ne plus suivre' }).click();
	await expect(page.getByRole('status')).toContainText('« Maison » n’est plus suivi.');
	expect(readFileSync(secrets, 'utf8')).not.toContain(TOKEN);
});
