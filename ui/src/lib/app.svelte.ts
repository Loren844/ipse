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
	switchMemory: '/app/switch'
};
