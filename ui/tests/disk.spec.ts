import { expect, test } from '@playwright/test';
import { execFileSync } from 'node:child_process';
import { IPSE, startIpse, type Running } from './ipse.ts';

// Whether the disk holding the memory is encrypted (issue #262, decision 0009). The
// system's answer is stood in for by IPSE_DISK_ENCRYPTION, as each runner's differs.
let ipse: Running;
test.afterEach(() => {
	ipse.stop();
	delete process.env.IPSE_DISK_ENCRYPTION;
});

test('a disk that is not encrypted: said once at home, with the steps in Settings', async ({ page }) => {
	process.env.IPSE_DISK_ENCRYPTION = 'off';
	ipse = await startIpse();
	await page.goto(ipse.url);
	const note = page.getByTestId('disk-note');
	await expect(note.getByRole('heading', { name: 'Chiffrement du disque : désactivé' })).toBeVisible();
	await expect(note).toContainText('n’est pas chiffré');
	await note.getByRole('button', { name: 'C’est noté' }).click();
	await expect(note).toBeHidden();
	await page.reload();
	await expect(page.getByRole('heading', { level: 1 })).toBeVisible();
	await expect(note).toBeHidden();

	await page.goto(`${ipse.origin}/settings`);
	const disk = page.getByTestId('disk');
	await expect(disk).toHaveAttribute('data-state', 'off');
	await expect(disk).toContainText('Désactivé');
	await expect(disk).toContainText(/Pour le chiffrer|chiffrer le disque \(LUKS\)/);
});

test('opening a sensitive category for the first time warns of a disk not encrypted', async ({ page }) => {
	process.env.IPSE_DISK_ENCRYPTION = 'off';
	ipse = await startIpse('Inès Roux', 'fr', false, (vault) => {
		execFileSync(IPSE, ['client', 'add', 'Claude Desktop', '--vault', vault]);
	});
	await page.goto(ipse.url);
	await page.goto(`${ipse.origin}/ais`);
	const claude = page.getByTestId('ai');
	await claude.getByRole('switch', { name: 'Claude Desktop peut lire : Santé' }).click();
	const health = page.getByRole('dialog', { name: 'Ouvrir « Santé » à Claude Desktop ?' });
	await expect(health).toContainText('n’est pas chiffré');
	await health.getByRole('button', { name: 'Ouvrir à Claude Desktop' }).click();
	await expect(claude.getByRole('switch', { name: 'Claude Desktop peut lire : Santé' })).toBeChecked();

	// Once said, not again.
	await claude.getByRole('switch', { name: 'Claude Desktop peut lire : Maison' }).click();
	const home = page.getByRole('dialog', { name: 'Ouvrir « Maison » à Claude Desktop ?' });
	await expect(home).toBeVisible();
	await expect(home).not.toContainText('n’est pas chiffré');
});

test('an encrypted disk is said once, without steps', async ({ page }) => {
	process.env.IPSE_DISK_ENCRYPTION = 'on';
	ipse = await startIpse('Inès Roux', 'en');
	await page.goto(ipse.url);
	const note = page.getByTestId('disk-note');
	await expect(note.getByRole('heading', { name: 'Disk encryption: on' })).toBeVisible();
	await expect(note).not.toContainText('To encrypt it');
});
