// See https://svelte.dev/docs/kit/types#app.d.ts
declare global {
	namespace App {}
	interface Window {
		/**
		 * Set by the desktop app in its window, before the page's own scripts run; the
		 * app adds its own settings once the page has loaded, then sends an
		 * `ipse-app-settings` event.
		 */
		ipseApp?: { version: string; settings?: { notifications: boolean; start: boolean } };
	}
}
export {};
