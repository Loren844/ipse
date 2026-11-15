// The core's JSON API, on the same local server. Types come from the Rust types
// (generated in ./types by `cargo test`), so the two never drift.
import type { AppInfo } from './types/AppInfo.ts';

export type { AppInfo };

/** The session ended (the server was restarted, or the cookie is gone). */
export class SessionEnded extends Error {}

async function get<T>(path: string): Promise<T> {
	const response = await fetch(path, { headers: { accept: 'application/json' } });
	if (response.status === 401 || response.status === 403) throw new SessionEnded();
	if (!response.ok) throw new Error(`${path}: ${response.status}`);
	return (await response.json()) as T;
}

export const api = {
	app: () => get<AppInfo>('/api/app')
};
