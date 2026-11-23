import { api, SessionEnded, type AppInfo } from './api.ts';
import { i18n } from './i18n.svelte.ts';

type State =
	| { kind: 'loading' }
	| { kind: 'ready'; info: AppInfo }
	| { kind: 'ended' }
	| { kind: 'failed' };

class App {
	state: State = $state({ kind: 'loading' });

	async load() {
		try {
			const info = await api.app();
			i18n.useVaultLanguage(info.language);
			this.state = { kind: 'ready', info };
		} catch (error) {
			this.state = error instanceof SessionEnded ? { kind: 'ended' } : { kind: 'failed' };
		}
	}

	/** Reads the frame's figures again (the waiting count) without leaving the page. */
	async refresh() {
		try {
			const info = await api.app();
			if (this.state.kind === 'ready') this.state = { kind: 'ready', info };
		} catch (error) {
			if (error instanceof SessionEnded) this.state = { kind: 'ended' };
		}
	}
}

export const app = new App();

/** Whether the interface runs in the desktop app's window rather than in a browser. */
export const inApp = typeof window !== 'undefined' && window.ipseApp !== undefined;

/** Addresses the desktop app answers itself, in its window. */
export const appLinks = {
	/** Back to the app's first screen, to open or create another memory. */
	switchMemory: '/app/switch',
	/** The app's own settings: `?notifications=on`, `?start=off`. */
	settings: '/app/settings'
};

/** The desktop app's own settings: system notifications, starting with the computer. */
export type AppSettings = { notifications: boolean; start: boolean };

/** The app's own settings, as the app last gave them to the page; `null` until it has. */
class AppSettingsOfTheApp {
	value: AppSettings | null = $state(null);

	constructor() {
		if (!inApp) return;
		const read = () => (this.value = window.ipseApp?.settings ?? null);
		read();
		window.addEventListener('ipse-app-settings', read);
	}

	/** Asks the app to change `name`: it answers the address itself and gives the page the settings again. */
	change(name: keyof AppSettings, on: boolean) {
		location.href = `${appLinks.settings}?${name}=${on ? 'on' : 'off'}`;
	}
}

export const appSettings = new AppSettingsOfTheApp();
