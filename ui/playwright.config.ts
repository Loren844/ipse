import { defineConfig } from '@playwright/test';

// The interface is tested as the person uses it: `ipse ui` started on an invented
// memory, in a real browser. Build `ipse` first (`cargo build -p ipse`).
// The large memory's measurement (graph-scale) runs alone, after the others: writing
// and reading 22 000 files would slow the tests that time how fast a screen opens.
const SCALE = /graph-scale\.spec\.ts/;
export default defineConfig({
	testDir: 'tests',
	timeout: 30_000,
	retries: 0,
	reporter: [['list']],
	use: { browserName: 'chromium', locale: 'fr-FR' },
	projects: [
		{ name: 'interface', testIgnore: SCALE },
		{ name: 'scale', testMatch: SCALE, dependencies: ['interface'] }
	]
});
