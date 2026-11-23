// See https://svelte.dev/docs/kit/types#app.d.ts
declare global {
	namespace App {}
	interface Window {
		/**
		 * Set by the desktop app in its window, before the page's own scripts run; the
		 * app adds its own settings once the page has loaded, then sends an
		 * `ipse-app-settings` event. A new version it found comes as `update`, with an
		 * `ipse-app-update` event (app/src/updates.rs).
		 */
		ipseApp?: {
			version: string;
			/** `updates` is `null` in a build that cannot update itself. */
			settings?: { notifications: boolean; start: boolean; updates?: boolean | null };
			update?: { version: string; notes: string | null; format_changes: boolean } | { failed: true } | null;
		};
	}
}
export {};
