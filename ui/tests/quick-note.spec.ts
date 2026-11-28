import { expect, test } from '@playwright/test';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { startIpse, type Running } from './ipse.ts';

// A quick note, without an AI (issue #277): one line on a person's, place's or event's
// page, saved at once as the person's, with today's date.
const AARON = `---
id: 01jcx8m2p4q6r8t0v2w4y6z811
type: person
category: people
name: Aaron Petit
created: 2026-01-10T09:00:00+01:00
updated: 2026-01-10T09:00:00+01:00
---

## Faits

- Aime le jazz ^f-ab12
`;

const DINNER = `---
id: 01jcx8m2p4q6r8t0v2w4y6z812
type: event
category: agenda
created: 2026-11-12T19:30:00+01:00
updated: 2026-11-12T19:30:00+01:00
title: Dîner chez Aaron
start: 2026-11-14T20:00:00+01:00
---
`;

const today = () => new Intl.DateTimeFormat('en-CA', { timeZone: 'Europe/Paris' }).format(new Date());

let ipse: Running;
test.beforeEach(async () => {
	ipse = await startIpse('Inès Roux', 'fr', false, (vault) => {
		writeFileSync(join(vault, 'people/aaron-petit.md'), AARON);
		mkdirSync(join(vault, 'events/2026'), { recursive: true });
		writeFileSync(join(vault, 'events/2026/2026-11-14-diner-chez-aaron.md'), DINNER);
	});
});
test.afterEach(() => ipse.stop());

const read = (file: string) => readFileSync(join(ipse.vault, file), 'utf8');

for (const scheme of ['light', 'dark'] as const) {
	test(`a note on a person and on an event, ${scheme}`, async ({ page }) => {
		await page.emulateMedia({ colorScheme: scheme });
		await page.goto(ipse.url);
		await page.goto(`${ipse.origin}/memory/people/01jcx8m2p4q6r8t0v2w4y6z811`);
		const field = page.getByRole('textbox', { name: 'Noter quelque chose sur Aaron Petit' });
		await expect(page.getByRole('button', { name: 'Noter' })).toBeDisabled();
		await field.fill('Part vivre à Lyon en mars.');
		await field.press('Enter');
		await expect(page.getByRole('status')).toContainText('Noté, avec la date du jour.');
		await expect(page.locator('ul.facts')).toContainText(`${today()} : Part vivre à Lyon en mars.`);
		await expect(field).toHaveValue('');
		await expect(field).toBeFocused();
		expect(read('people/aaron-petit.md')).toContain(`- ${today()} : Part vivre à Lyon en mars. ^f-`);

		// In the history, as the person's own change.
		await page.goto(`${ipse.origin}/history`);
		await expect(page.getByText('Aaron Petit').first()).toBeVisible();

		await page.goto(`${ipse.origin}/agenda/01jcx8m2p4q6r8t0v2w4y6z812`);
		const onEvent = page.getByRole('textbox', { name: 'Noter quelque chose sur Dîner chez Aaron' });
		await onEvent.fill('Aaron a adoré le dessert.');
		await page.getByRole('button', { name: 'Noter' }).click();
		await expect(page.locator('p.notes')).toContainText(`${today()} : Aaron a adoré le dessert.`);
		expect(read('events/2026/2026-11-14-diner-chez-aaron.md')).toContain(`## Notes\n\n${today()} : Aaron a adoré le dessert.\n`);
	});
}

const notes = [
	{ language: 'fr', scheme: 'light' as const, field: 'Noter quelque chose pour aujourd’hui', saved: 'Noté dans la note du jour.', of: 'Note du', back: 'Liens vers ici' },
	{ language: 'en', scheme: 'dark' as const, field: 'Note something for today', saved: 'Added to the note of the day.', of: 'Note of', back: 'Links here' }
];

for (const t of notes) {
	test.describe(`note of the day, ${t.language}, ${t.scheme}`, () => {
		let day: Running;
		test.beforeEach(async () => {
			day = await startIpse('Inès Roux', t.language, false, (vault) => writeFileSync(join(vault, 'people/aaron-petit.md'), AARON));
		});
		test.afterEach(() => day.stop());

		test('a line on the home screen, a mention with @', async ({ page }) => {
			await page.emulateMedia({ colorScheme: t.scheme });
			await page.goto(day.url);
			await page.goto(`${day.origin}/?note`);
			const field = page.getByRole('combobox', { name: t.field });
			await expect(field).toBeFocused();
			await field.pressSequentially('Vu @Aar');
			const option = page.getByRole('option', { name: /Aaron Petit/ });
			await expect(option).toBeVisible();
			await field.press('Enter');
			await expect(field).toHaveValue('Vu @Aaron Petit ');
			await field.pressSequentially('au marché.');
			await field.press('Enter');
			await expect(page.getByRole('status')).toContainText(t.saved);
			const lines = page.getByTestId('day-note').locator('li');
			await expect(lines).toHaveCount(1);
			await expect(lines.first()).toContainText(/^\d\d:\d\d ?: Vu .*Aaron Petit au marché\.$/);
			await expect(lines.first().getByRole('link', { name: 'Aaron Petit' })).toBeVisible();

			const file = readFileSync(join(day.vault, `notes/${today().slice(0, 4)}/${today()}.md`), 'utf8');
			expect(file).toContain('type: note');
			expect(file).toMatch(/- \d\d:\d\d ?: Vu \[\[aaron-petit\|Aaron Petit\]\] au marché\.\n/);

			// Aaron's page shows the note among what links to him, and opens it.
			await lines.first().getByRole('link', { name: 'Aaron Petit' }).click();
			const back = page.getByTestId('backlinks');
			await expect(back.getByRole('link', { name: new RegExp(t.of) })).toBeVisible();
			await back.getByRole('link', { name: new RegExp(t.of) }).click();
			await expect(page.getByTestId('day-note').locator('li')).toContainText('au marché');
		});
	});
}
