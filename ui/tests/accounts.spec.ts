// A calendars and contacts account on the Sources screen (issue #154): connect it, choose
// what to follow, see its events and contacts, disconnect it. A small WebDAV server on
// this computer stands for the account. Data invented.
import { expect, test } from '@playwright/test';
import { createServer, type Server } from 'node:http';
import { mkdtempSync, readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { startIpse, type Running } from './ipse.ts';

const PASSWORD = 'app-pass-2b7d4e';
const CREDENTIALS = `Basic ${Buffer.from(`ines:${PASSWORD}`).toString('base64')}`;

const day = (days: number) => {
	const d = new Date(Date.now() + days * 86_400_000);
	return d.toISOString().slice(0, 10).replaceAll('-', '');
};
const EVENT = [
	'BEGIN:VCALENDAR',
	'VERSION:2.0',
	'BEGIN:VEVENT',
	'UID:atelier@test',
	'SUMMARY:Atelier de reliure',
	`DTSTART;VALUE=DATE:${day(3)}`,
	`DTEND;VALUE=DATE:${day(4)}`,
	'END:VEVENT',
	'END:VCALENDAR',
	''
].join('\r\n');
const CARD = ['BEGIN:VCARD', 'VERSION:3.0', 'UID:gaspard@test', 'FN:Gaspard Lemoine', 'END:VCARD', ''].join('\r\n');

const ITEMS: Record<string, { etag: string; data: string; collection: string }> = {
	'/dav/cal/perso/atelier.ics': { etag: '"1"', data: EVENT, collection: '/dav/cal/perso/' },
	'/dav/card/amis/gaspard.vcf': { etag: '"1"', data: CARD, collection: '/dav/card/amis/' }
};

const ok = (href: string, props: string) =>
	`<d:response><d:href>${href}</d:href><d:propstat><d:prop>${props}</d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response>`;
const all = (responses: string) =>
	`<?xml version="1.0"?><d:multistatus xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav" xmlns:a="urn:ietf:params:xml:ns:carddav" xmlns:cs="http://calendarserver.org/ns/">${responses}</d:multistatus>`;
const escape = (text: string) => text.replaceAll('&', '&amp;').replaceAll('<', '&lt;');

/** What the account answers to `method` on `path`. */
function answer(method: string, path: string, body: string): string | null {
	if (method === 'PROPFIND' && path.startsWith('/.well-known/'))
		return all(ok(path, '<d:current-user-principal><d:href>/dav/me/</d:href></d:current-user-principal>'));
	if (method === 'PROPFIND' && path === '/dav/me/')
		return all(
			ok(
				path,
				'<c:calendar-home-set><d:href>/dav/cal/</d:href></c:calendar-home-set><a:addressbook-home-set><d:href>/dav/card/</d:href></a:addressbook-home-set>'
			)
		);
	if (method === 'PROPFIND' && path === '/dav/cal/')
		return all(
			ok(
				'/dav/cal/perso/',
				'<d:resourcetype><d:collection/><c:calendar/></d:resourcetype><d:displayname>Perso</d:displayname><d:sync-token>t1</d:sync-token>'
			) +
				ok(
					'/dav/cal/travail/',
					'<d:resourcetype><d:collection/><c:calendar/></d:resourcetype><d:displayname>Travail</d:displayname><d:sync-token>t1</d:sync-token>'
				)
		);
	if (method === 'PROPFIND' && path === '/dav/card/')
		return all(
			ok('/dav/card/amis/', '<d:resourcetype><d:collection/><a:addressbook/></d:resourcetype><d:displayname>Amis</d:displayname><cs:getctag>1</cs:getctag>')
		);
	const items = Object.entries(ITEMS).filter(([, item]) => item.collection === path);
	if (method === 'PROPFIND') return all(items.map(([href, item]) => ok(href, `<d:getetag>${item.etag}</d:getetag><d:resourcetype/>`)).join(''));
	if (method === 'REPORT') {
		const data = body.includes('calendar-multiget') ? 'c:calendar-data' : 'a:address-data';
		return all(
			items
				.filter(([href]) => body.includes(href))
				.map(([href, item]) => ok(href, `<d:getetag>${item.etag}</d:getetag><${data}>${escape(item.data)}</${data}>`))
				.join('')
		);
	}
	return null;
}

let ipse: Running;
let server: Server;
let secrets: string;
let address = '';

test.beforeEach(async () => {
	// A test machine has no keychain: a development build keeps the password in this file.
	secrets = join(mkdtempSync(join(tmpdir(), 'ipse-secrets-')), 'secrets.json');
	process.env.IPSE_TEST_SECRETS = secrets;
	server = createServer((request, response) => {
		let body = '';
		request.on('data', (chunk) => (body += chunk));
		request.on('end', () => {
			if (request.headers.authorization !== CREDENTIALS) {
				response.writeHead(401);
				response.end();
				return;
			}
			const xml = answer(request.method ?? '', request.url ?? '', body);
			response.writeHead(xml === null ? 404 : 207, { 'content-type': 'application/xml; charset=utf-8' });
			response.end(xml ?? '');
		});
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

test('connect an account, choose what to follow, then disconnect it', async ({ page }) => {
	await page.goto(ipse.url);
	await page.getByRole('link', { name: 'Sources' }).first().click();
	await page.getByRole('link', { name: 'Un compte d’agendas et de contacts' }).click();
	const form = page.getByRole('region', { name: 'Connecter un compte d’agendas et de contacts' });

	await form.getByLabel('Nom du compte').fill('Nuage');
	await form.getByLabel('Adresse du serveur').fill(address);
	await form.getByLabel('Nom d’utilisateur').fill('ines');
	await form.getByLabel('Mot de passe d’application').fill('not-the-password');
	await form.getByRole('button', { name: 'Connecter ce compte' }).click();
	await expect(page.getByRole('status')).toContainText('Le serveur a refusé le nom d’utilisateur ou le mot de passe');
	expect(readFileSync(secrets, 'utf8')).not.toContain('not-the-password');

	await form.getByLabel('Mot de passe d’application').fill(PASSWORD);
	await form.getByRole('button', { name: 'Connecter ce compte' }).click();
	await expect(page.getByRole('status')).toHaveText('« Nuage » est connecté. Choisissez les agendas et les carnets d’adresses à suivre.');
	// The password leaves the page, and is kept only in the keychain (here, the test file).
	await expect(form).toHaveCount(0);
	expect(readFileSync(secrets, 'utf8')).toContain(PASSWORD);
	expect(JSON.stringify(await page.evaluate(() => fetch('/api/sources').then((r) => r.json())))).not.toContain(PASSWORD);

	// Nothing is read before the person chooses.
	const source = page.getByTestId('sources').getByRole('listitem').filter({ hasText: 'Nuage' });
	await expect(source.getByRole('heading', { name: 'Choisissez ce qu’Ipse suit' })).toBeVisible();
	await source.getByLabel('Perso').check();
	await source.getByLabel('Amis').check();
	await expect(source.getByLabel('Travail')).not.toBeChecked();
	await source.getByRole('button', { name: 'Suivre la sélection' }).click();
	await expect(page.getByRole('status')).toHaveText('Synchronisé.');
	await expect(source.getByText('Agendas et carnets d’adresses suivis')).toBeVisible();

	await page.getByRole('link', { name: 'Agenda' }).first().click();
	await expect(page.getByText('Atelier de reliure').first()).toBeVisible();
	await page.getByRole('link', { name: 'Sources' }).first().click();

	await source.getByRole('button', { name: 'Ne plus suivre' }).click();
	const dialog = page.getByRole('dialog', { name: 'Ne plus suivre « Nuage » ?' });
	await expect(dialog).toContainText('oubliera son mot de passe');
	await dialog.getByRole('button', { name: 'Ne plus suivre' }).click();
	await expect(page.getByRole('status')).toContainText('« Nuage » n’est plus suivi.');
	expect(readFileSync(secrets, 'utf8')).not.toContain(PASSWORD);
});
