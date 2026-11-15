// The core's JSON API, on the same local server. Types come from the Rust types
// (generated in ./types by `cargo test`), so the two never drift.
import type { AppInfo } from './types/AppInfo.ts';
import type { EventsView } from './types/EventsView.ts';
import type { FileView } from './types/FileView.ts';
import type { HomeView } from './types/HomeView.ts';
import type { PeopleView } from './types/PeopleView.ts';

export type { AppInfo, EventsView, FileView, HomeView, PeopleView };
export type { EventView } from './types/EventView.ts';
export type { EntityView } from './types/EntityView.ts';
export type { LinkView } from './types/LinkView.ts';

/** The session ended (the server was restarted, or the cookie is gone). */
export class SessionEnded extends Error {}

/** The answer for a file that does not exist. */
export class NotFound extends Error {}

async function get<T>(path: string, params: Record<string, string | number | undefined> = {}): Promise<T> {
	const query = new URLSearchParams();
	for (const [key, value] of Object.entries(params)) if (value !== undefined && value !== '') query.set(key, String(value));
	const url = query.size ? `${path}?${query}` : path;
	const response = await fetch(url, { headers: { accept: 'application/json' } });
	if (response.status === 401 || response.status === 403) throw new SessionEnded();
	if (response.status === 404) throw new NotFound();
	if (!response.ok) throw new Error(`${path}: ${response.status}`);
	return (await response.json()) as T;
}

export const api = {
	app: () => get<AppInfo>('/api/app'),
	home: () => get<HomeView>('/api/home'),
	people: (q?: string, offset?: number) => get<PeopleView>('/api/people', { q, offset }),
	events: (from?: string, to?: string) => get<EventsView>('/api/events', { from, to }),
	file: (id: string) => get<FileView>(`/api/file/${encodeURIComponent(id)}`)
};
