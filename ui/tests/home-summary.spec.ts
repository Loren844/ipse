import { expect, test } from '@playwright/test';
import { mkdirSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { startIpse, type Running } from './ipse.ts';

// The home screen shows a summary only while it is recent and says more than the
// agenda (issue #261): an old one, or one short line, stays in the summaries' screen.

/** The day `back` days before today in Paris, `YYYY-MM-DD`. */
function daysAgo(back: number): string {
	const paris = new Date().toLocaleDateString('en-CA', { timeZone: 'Europe/Paris' });
	const day = new Date(`${paris}T12:00:00Z`);
	day.setUTCDate(day.getUTCDate() - back);
	return day.toISOString().slice(0, 10);
}

/** The Monday of the week holding `day`. */
function monday(day: string): string {
	const d = new Date(`${day}T12:00:00Z`);
	d.setUTCDate(d.getUTCDate() - ((d.getUTCDay() + 6) % 7));
	return d.toISOString().slice(0, 10);
}

function isoWeek(day: string): string {
	const d = new Date(`${day}T12:00:00Z`);
	d.setUTCDate(d.getUTCDate() + 3 - ((d.getUTCDay() + 6) % 7));
	const year = d.getUTCFullYear();
	const first = new Date(Date.UTC(year, 0, 4));
	const week = 1 + Math.round((d.getTime() - first.getTime()) / 86_400_000 / 7 - (3 - ((first.getUTCDay() + 6) % 7)) / 7);
	return `${year}-W${String(week).padStart(2, '0')}`;
}

let n = 0;
function summary(vault: string, period: 'day' | 'week', start: string, text: string) {
	const label = period === 'day' ? start : isoWeek(start);
	const file = join(vault, 'summaries', period, label.slice(0, 4), `${label}.md`);
	mkdirSync(dirname(file), { recursive: true });
	n += 1;
	const id = `01jd3k8m2p4q6r8t0v2w4y6z${String(n).padStart(2, '0')}`;
	writeFileSync(
		file,
		`---\nid: ${id}\ntype: summary\ncategory: summaries\ncreated: ${start}T23:00:00+01:00\nupdated: ${start}T23:00:00+01:00\nperiod: ${period}\nstart: ${start}\n---\n\n## Résumé\n\n${text}\n`
	);
}

const FULL = 'Une longue marche au bord du lac avec Julie.\nLe soir, un film et un dîner tranquille à la maison.';

let ipse: Running;
test.beforeEach(async () => {
	ipse = await startIpse('Inès Roux', 'fr');
});
test.afterEach(() => ipse.stop());

test('an old summary, or one short line, is not on the home screen', async ({ page }) => {
	const latest = page.getByRole('region', { name: 'Dernier résumé' });
	const home = async () => {
		await page.goto(ipse.origin);
		await expect(page.getByRole('heading', { name: 'Ce qui vient' })).toBeVisible();
	};
	await page.goto(ipse.url);

	// Ten days old: not shown, however full.
	summary(ipse.vault, 'day', daysAgo(10), FULL);
	await home();
	await expect(latest).toHaveCount(0);

	// Yesterday's, one short line from the agenda: not shown either.
	summary(ipse.vault, 'day', daysAgo(1), 'Une journée tranquille : yoga.');
	await home();
	await expect(latest).toHaveCount(0);

	// Last week's: shown when no day says more.
	summary(ipse.vault, 'week', monday(daysAgo(7)), 'Une semaine de yoga et d’amis.\nEntre deux averses, un week-end au calme.');
	await home();
	await expect(latest).toContainText('Une semaine de yoga et d’amis');

	// A full day three days ago comes first.
	summary(ipse.vault, 'day', daysAgo(3), FULL);
	await home();
	await expect(latest).toContainText('Une longue marche au bord du lac avec Julie.');
	await expect(latest).not.toContainText('yoga');
});
