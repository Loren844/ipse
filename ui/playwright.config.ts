import { defineConfig } from '@playwright/test';

// The interface is tested as the person uses it: `ipse ui` started on an invented
// memory, in a real browser. Build `ipse` first (`cargo build -p ipse`).
// The large memory's measurement (graph-scale) runs alone, after the others: writing
// and reading 22 000 files would slow the tests that time how fast a screen opens.
const SCALE = /graph-scale\.spec\.ts/;
// The desktop app shows the interface in WebKit on macOS. IPSE_WEBKIT=1 runs the suite
// there too (CI does): `npx playwright install webkit` first.
const webkit = process.env.IPSE_WEBKIT
	? [{ name: 'webkit', testIgnore: SCALE, use: { browserName: 'webkit' as const } }]
	: [];
export default defineConfig({
	testDir: 'tests',
	timeout: 30_000,
	retries: 0,
	reporter: [['list']],
	// Without motion, as a person who asks their system for less of it: a test never
	// waits on a transition. motion.spec.ts checks the motion itself.
	use: { browserName: 'chromium', locale: 'fr-FR', contextOptions: { reducedMotion: 'reduce' } },
	projects: [
		{ name: 'interface', testIgnore: SCALE },
		{ name: 'scale', testMatch: SCALE, dependencies: ['interface'] },
		...webkit
	]
});
