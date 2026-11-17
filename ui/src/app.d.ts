// See https://svelte.dev/docs/kit/types#app.d.ts
declare global {
	namespace App {}
	interface Window {
		/** Set by the desktop app in its window, before the page's own scripts run. */
		ipseApp?: { version: string };
	}
}
export {};
