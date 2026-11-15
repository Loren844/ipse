import { defineConfig } from '@playwright/test';

// The interface is tested as the person uses it: `ipse ui` started on an invented
// memory, in a real browser. Build `ipse` first (`cargo build -p ipse`).
export default defineConfig({
	testDir: 'tests',
	timeout: 30_000,
	retries: 0,
	reporter: [['list']],
	use: { browserName: 'chromium', locale: 'fr-FR' }
});
