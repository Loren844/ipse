// One screen's data, fetched from the API: loading, there, missing, or failed.
import { NotFound, SessionEnded } from './api.ts';
import { app } from './app.svelte.ts';

export type Remote<T> =
	| { kind: 'loading' }
	| { kind: 'ready'; value: T }
	| { kind: 'missing' }
	| { kind: 'failed' };

export class Loader<T> {
	state: Remote<T> = $state({ kind: 'loading' });
	#ticket = 0;

	/** Fetches with `fetcher`; an older fetch that ends later is ignored. */
	async load(fetcher: () => Promise<T>, keep = false) {
		const ticket = ++this.#ticket;
		if (!keep) this.state = { kind: 'loading' };
		try {
			const value = await fetcher();
			if (ticket === this.#ticket) this.state = { kind: 'ready', value };
		} catch (error) {
			if (ticket !== this.#ticket) return;
			if (error instanceof SessionEnded) app.state = { kind: 'ended' };
			this.state = error instanceof NotFound ? { kind: 'missing' } : { kind: 'failed' };
		}
	}
}
