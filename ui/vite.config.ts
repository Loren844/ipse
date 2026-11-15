import adapter from '@sveltejs/adapter-static';
import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';

export default defineConfig({
	plugins: [
		sveltekit({
			// A single-page app: `ipse ui` serves index.html for every page.
			adapter: adapter({ pages: 'build', assets: 'build', fallback: 'index.html', strict: true }),
			// Hashes of the inline start-up script, so that the page runs under a strict
			// Content-Security-Policy that allows no other inline script.
			csp: { mode: 'hash', directives: { 'script-src': ['self'] } },
			output: { bundleStrategy: 'single' },
			version: { name: 'ipse' }
		})
	],
	build: { assetsInlineLimit: 0, reportCompressedSize: false }
});
