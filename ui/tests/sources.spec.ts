// The Sources screen (issue #152): import a file, follow a calendar by its link, sync
// it, stop following it. Data invented (tests/fixtures).
import { expect, test } from '@playwright/test';
import { createServer, type Server } from 'node:http';
import { mkdtempSync, readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { startIpse, type Running } from './ipse.ts';

const FIXTURES = resolve(import.meta.dirname, '../../crates/ipse/tests/fixtures');

let ipse: Running;
let calendar: Server;
let secrets: string;
let link = '';
/** What the calendar answers: 200 with the file, or this status. */
let status = 200;

test.beforeEach(async () => {
	// A test machine has no keychain: a development build keeps the link in this file.
	secrets = join(mkdtempSync(join(tmpdir(), 'ipse-secrets-')), 'secrets.json');
	process.env.IPSE_TEST_SECRETS = secrets;
	const ics = readFileSync(join(FIXTURES, 'calendar/google.ics'));
	calendar = createServer((request, response) => {
		if (request.url === '/basic.ics' && status !== 200) {
			response.writeHead(status);
			response.end();
		} else if (request.url === '/basic.ics') {
			response.writeHead(200, { 'content-type': 'text/calendar' });
			response.end(ics);
		} else {
			response.writeHead(404);
			response.end();
		}
	});
	await new Promise<void>((done) => calendar.listen(0, '127.0.0.1', done));
	const address = calendar.address();
	link = `http://127.0.0.1:${typeof address === 'object' && address ? address.port : 0}/basic.ics`;
	ipse = await startIpse('Inès Roux', 'fr');
});
test.afterEach(() => {
	status = 200;
	ipse.stop();
	calendar.close();
	delete process.env.IPSE_TEST_SECRETS;
});

test('import a contacts file, then see it among the sources', async ({ page }) => {
	await page.goto(ipse.url);
	await page.getByRole('link', { name: 'Sources' }).first().click();
	await expect(page.getByRole('heading', { level: 1 })).toHaveText('Sources');
	await expect(page.getByText('Rien encore. Importez un fichier ou suivez un agenda pour commencer.')).toBeVisible();

	await page.getByLabel('choisissez-le').setInputFiles(join(FIXTURES, 'contacts/google-v3.vcf'));
	await expect(page.getByRole('status')).toHaveText('Fichier importé.');
	const source = page.getByTestId('sources').getByRole('listitem').filter({ hasText: 'Contacts' });
	await expect(source).toContainText('Importé à l’instant.');
	await expect(source).toContainText('fiches ajoutées');

	// The people are in the memory.
	await page.getByRole('link', { name: 'Mémoire' }).first().click();
	await expect(page.getByText('Personnes')).toBeVisible();

	// Location history, from a phone's export.
	await page.getByRole('link', { name: 'Sources' }).first().click();
	await page.getByLabel('choisissez-le').setInputFiles(join(FIXTURES, 'location/Timeline.json'));
	await expect(page.getByRole('status')).toHaveText('Fichier importé.');
	await expect(page.getByTestId('sources').getByRole('listitem').filter({ hasText: 'Historique des positions' })).toBeVisible();

	// A file Ipse cannot read is said plainly, and nothing is listed for it.
	await page.getByLabel('choisissez-le').setInputFiles({ name: 'photo.jpg', mimeType: 'image/jpeg', buffer: Buffer.from('not a contact') });
	await expect(page.getByRole('status')).toContainText('Ipse ne sait pas encore lire ce type de fichier');
	await expect(page.getByTestId('sources').getByRole('listitem')).toHaveCount(2);
});

test('follow a calendar by its link, sync it, then stop following it', async ({ page }) => {
	await page.goto(ipse.url);
	await page.getByRole('link', { name: 'Sources' }).first().click();

	await page.getByLabel('Nom de l’agenda').fill('Perso');
	await page.getByLabel('Lien secret (iCal)').fill('ftp://example.org/agenda.ics');
	await page.getByRole('button', { name: 'Suivre cet agenda' }).click();
	await expect(page.getByRole('status')).toContainText('Ce lien ne peut pas être suivi');

	await page.getByLabel('Lien secret (iCal)').fill(link);
	await page.getByRole('button', { name: 'Suivre cet agenda' }).click();
	await expect(page.getByRole('status')).toHaveText('« Perso » est suivi.');
	// The link leaves the page, and is kept only in the keychain (here, the test file).
	await expect(page.getByLabel('Lien secret (iCal)')).toHaveValue('');
	expect(readFileSync(secrets, 'utf8')).toContain(link);
	expect(JSON.stringify(await page.evaluate(() => fetch('/api/sources').then((r) => r.json())))).not.toContain(link);

	const source = page.getByTestId('sources').getByRole('listitem').filter({ hasText: 'Perso' });
	await expect(source).toContainText('Suivi, toutes les 15 minutes');
	await expect(source).toContainText('fiches ajoutées');

	// The same link again: already followed.
	await page.getByLabel('Lien secret (iCal)').fill(link);
	await page.getByRole('button', { name: 'Suivre cet agenda' }).click();
	await expect(page.getByRole('status')).toHaveText('Cet agenda est déjà suivi, sous le nom « Perso ».');
	await page.getByLabel('Lien secret (iCal)').fill('');

	await source.getByRole('button', { name: 'Synchroniser' }).click();
	await expect(page.getByRole('status')).toHaveText('Synchronisé.');
	await expect(source).toContainText('déjà à jour');

	await source.getByRole('button', { name: 'Ne plus suivre' }).click();
	const dialog = page.getByRole('dialog', { name: 'Ne plus suivre « Perso » ?' });
	await expect(dialog).toContainText('Ses événements restent dans votre mémoire.');
	await dialog.getByRole('button', { name: 'Ne plus suivre' }).click();
	await expect(page.getByRole('status')).toHaveText('« Perso » n’est plus suivi. Ses événements restent dans votre mémoire.');
	expect(readFileSync(secrets, 'utf8')).not.toContain(link);
	// Its events stay: the agenda still lists the dinner.
	await page.getByRole('link', { name: 'Agenda' }).first().click();
	await expect(page.getByText('Dîner chez Julie').first()).toBeVisible();
});

test('a followed calendar that fails is marked beside Sources until it syncs again', async ({ page }) => {
	await page.goto(ipse.url);
	const nav = page.getByRole('navigation', { name: 'Sections' });
	await nav.getByRole('link', { name: 'Sources' }).click();
	await page.getByLabel('Nom de l’agenda').fill('Perso');
	await page.getByLabel('Lien secret (iCal)').fill(link);
	await page.getByRole('button', { name: 'Suivre cet agenda' }).click();
	await expect(page.getByRole('status')).toHaveText('« Perso » est suivi.');
	const failing = nav.getByRole('img', { name: 'Une source suivie ne se synchronise plus' });
	await expect(failing).toHaveCount(0);

	status = 503;
	const source = page.getByTestId('sources').getByRole('listitem').filter({ hasText: 'Perso' });
	await source.getByRole('button', { name: 'Synchroniser' }).click();
	await expect(failing).toBeVisible();

	status = 200;
	await source.getByRole('button', { name: 'Synchroniser' }).click();
	await expect(page.getByRole('status')).toHaveText('Synchronisé.');
	await expect(failing).toHaveCount(0);
});
