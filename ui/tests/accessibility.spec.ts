import AxeBuilder from '@axe-core/playwright';
import { expect, test, type BrowserContextOptions } from '@playwright/test';
import { startDemo, type Running } from './ipse.ts';

// Each main screen of the demo memory, in light and dark, checked by axe against WCAG
// 2.1 A and AA (issue #287): contrast, names of controls, landmarks, ARIA.
let ipse: Running;
// The one-time address opens one session, shared by every screen.
let session: BrowserContextOptions['storageState'];
test.beforeAll(async ({ browser }) => {
	ipse = await startDemo('fr');
	const context = await browser.newContext();
	const page = await context.newPage();
	await page.goto(ipse.url);
	await expect(page.getByTestId('memory-name')).toBeVisible();
	session = await context.storageState();
	await context.close();
});
test.afterAll(() => ipse.stop());

const screens = [
	'/',
	'/memory',
	'/memory/people',
	'/agenda',
	'/review',
	'/summaries',
	'/sources',
	'/ais',
	'/history',
	'/settings',
	'/how'
];

for (const scheme of ['light', 'dark'] as const) {
	for (const screen of screens) {
		test(`${screen} in ${scheme}`, async ({ browser }) => {
			const context = await browser.newContext({ storageState: session, colorScheme: scheme, locale: 'fr-FR', reducedMotion: 'reduce' });
			const page = await context.newPage();
			await page.goto(`${ipse.origin}${screen}`);
			await expect(page.getByRole('heading', { level: 1 })).toBeVisible();
			await page.waitForLoadState('networkidle');
			const { violations } = await new AxeBuilder({ page })
				.withTags(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa'])
				.analyze();
			const found = violations.map((v) => `${v.id} (${v.impact}): ${v.help}\n  ${v.nodes.map((n) => `${n.target.join(' ')}: ${n.failureSummary ?? ''}`).slice(0, 5).join('\n  ')}`);
			await context.close();
			expect(found, found.join('\n')).toEqual([]);
		});
	}
}
