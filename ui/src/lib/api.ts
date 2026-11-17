// The core's JSON API, on the same local server. Types come from the Rust types
// (generated in ./types by `cargo test`), so the two never drift.
import type { AppInfo } from './types/AppInfo.ts';
import type { EventsView } from './types/EventsView.ts';
import type { FileView } from './types/FileView.ts';
import type { HomeView } from './types/HomeView.ts';
import type { PeopleView } from './types/PeopleView.ts';
import type { DecisionView } from './types/DecisionView.ts';
import type { InboxView } from './types/InboxView.ts';
import type { ProposalView } from './types/ProposalView.ts';

export type { AppInfo, DecisionView, EventsView, FileView, HomeView, InboxView, PeopleView, ProposalView };
export type { ProposalLine } from './types/ProposalLine.ts';
export type { AisView } from './types/AisView.ts';
export type { AiView } from './types/AiView.ts';
export type { CategoryView } from './types/CategoryView.ts';
export type { ConnectedView } from './types/ConnectedView.ts';
export type { AppView } from './types/AppView.ts';
import type { AisView } from './types/AisView.ts';
import type { MapView } from './types/MapView.ts';
export type { MapView };
export type { MapPlaceView } from './types/MapPlaceView.ts';
import type { TimelineView } from './types/TimelineView.ts';
export type { TimelineView };
export type { YearView } from './types/YearView.ts';
export type { MonthView } from './types/MonthView.ts';
import type { ConnectedView } from './types/ConnectedView.ts';
import type { SourcesChange } from './types/SourcesChange.ts';
import type { SourcesView } from './types/SourcesView.ts';
export type { SourcesChange, SourcesView };
export type { SourceView } from './types/SourceView.ts';
import type { HistoryView } from './types/HistoryView.ts';
import type { AccessesView } from './types/AccessesView.ts';
import type { RunsView } from './types/RunsView.ts';
import type { DiagnosticsView } from './types/DiagnosticsView.ts';
import type { UndoneView } from './types/UndoneView.ts';
import type { SummariesView } from './types/SummariesView.ts';
import type { SummaryView } from './types/SummaryView.ts';
export type { SummariesView, SummaryView };
import type { QuestionsView } from './types/QuestionsView.ts';
import type { SettingsView } from './types/SettingsView.ts';
export type { SettingsView };
import type { MemoryView } from './types/MemoryView.ts';
import type { ListView } from './types/ListView.ts';
import type { SearchView } from './types/SearchView.ts';
export type { MemoryView, ListView, SearchView };
export type { StateView } from './types/StateView.ts';
export type { FoundView } from './types/FoundView.ts';
export type { QuestionsView };
export type { QuestionView } from './types/QuestionView.ts';
export type { HistoryView, AccessesView, RunsView, DiagnosticsView };
export type { ChangeLine } from './types/ChangeLine.ts';
export type { RunView } from './types/RunView.ts';
export type { FileChangesView } from './types/FileChangesView.ts';
export type { ChangeView } from './types/ChangeView.ts';
export type { EventView } from './types/EventView.ts';
export type { EntityView } from './types/EntityView.ts';
export type { LinkView } from './types/LinkView.ts';
import type { EditView } from './types/EditView.ts';
import type { SavedView } from './types/SavedView.ts';
export type { EditView, SavedView };
import type { GraphView } from './types/GraphView.ts';
export type { GraphView };
export type { NodeView } from './types/NodeView.ts';
export type { LinkField } from './types/LinkField.ts';
export type { RelationField } from './types/RelationField.ts';

/** What the form sends to save a file: the keys it shows, facts by their place in the
 * form as read, relations and notes. */
export type Draft = {
	keys: Record<string, string | string[] | null>;
	facts?: { was: number | null; text: string }[];
	relations?: { word: string; target: string; display?: string | null }[];
	notes?: string;
};

/** The session ended (the server was restarted, or the cookie is gone). */
export class SessionEnded extends Error {}

/** The answer for a file that does not exist. */
export class NotFound extends Error {}

/** A decision that no longer fits the memory: already decided, changed since, or a value
 * Ipse cannot keep. Its message says why, in English, when the server gave one. */
export class Conflict extends Error {}

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

/** Sends a decision as JSON; the server checks it comes from this page. */
async function post<T>(path: string, body: unknown = {}): Promise<T> {
	const response = await fetch(path, {
		method: 'POST',
		headers: { accept: 'application/json', 'content-type': 'application/json' },
		body: JSON.stringify(body)
	});
	if (response.status === 401 || response.status === 403) throw new SessionEnded();
	if (response.status === 404) throw new NotFound();
	if (response.status === 409) {
		const answer = (await response.json().catch(() => ({}))) as { error?: string };
		throw new Conflict(answer.error ?? '');
	}
	if (!response.ok) throw new Error(`${path}: ${response.status}`);
	return (await response.json()) as T;
}

/** Sends a file to import, as it is; the server writes it to disk as it arrives. */
async function upload(file: File): Promise<SourcesChange> {
	const response = await fetch(`/api/sources/import?${new URLSearchParams({ name: file.name })}`, {
		method: 'POST',
		headers: { accept: 'application/json', 'content-type': 'application/octet-stream' },
		body: file
	});
	if (response.status === 401 || response.status === 403) throw new SessionEnded();
	if (!response.ok) throw new Error(`import: ${response.status}`);
	return (await response.json()) as SourcesChange;
}

const proposal = (id: string) => `/api/proposals/${encodeURIComponent(id)}`;

export const api = {
	app: () => get<AppInfo>('/api/app'),
	home: () => get<HomeView>('/api/home'),
	people: (q?: string, offset?: number) => get<PeopleView>('/api/people', { q, offset }),
	events: (from?: string, to?: string) => get<EventsView>('/api/events', { from, to }),
	file: (id: string) => get<FileView>(`/api/file/${encodeURIComponent(id)}`),
	proposals: () => get<InboxView>('/api/proposals'),
	proposal: (id: string) => get<ProposalView>(proposal(id)),
	accept: (id: string) => post<DecisionView>(`${proposal(id)}/accept`),
	acceptAll: (ids: string[]) => post<DecisionView>('/api/proposals/accept', { ids }),
	refuse: (id: string, comment?: string) => post<DecisionView>(`${proposal(id)}/refuse`, { comment: comment || undefined }),
	undo: (id: string) => post<DecisionView>(`${proposal(id)}/undo`),
	ais: () => get<AisView>('/api/ais'),
	connect: (name: string, local: boolean, openAll: boolean, install?: string) =>
		post<ConnectedView>('/api/ais', { name, local, open_all: openAll, install }),
	setRights: (id: string, read: string[], propose: string[]) =>
		post<AisView>(`/api/ais/${encodeURIComponent(id)}/rights`, { read, propose }),
	disconnect: (id: string) => post<AisView>(`/api/ais/${encodeURIComponent(id)}/remove`),
	addCategory: (label: string, sensitive: boolean, openTo: string[]) =>
		post<AisView>('/api/categories', { label, sensitive, open_to: openTo }),
	history: (offset?: number) => get<HistoryView>('/api/history', { offset }),
	undoChange: (id: string) => post<UndoneView>(`/api/history/${encodeURIComponent(id)}/undo`),
	accesses: () => get<AccessesView>('/api/accesses'),
	runs: () => get<RunsView>('/api/runs'),
	map: () => get<MapView>('/api/map'),
	timeline: () => get<TimelineView>('/api/timeline'),
	sources: () => get<SourcesView>('/api/sources'),
	importFile: upload,
	followCalendar: (link: string, name: string) => post<SourcesChange>('/api/sources/follow', { link, name }),
	syncSource: (instance: string) => post<SourcesChange>('/api/sources/sync', { instance }),
	unfollow: (instance: string) => post<SourcesChange>('/api/sources/unfollow', { instance }),
	diagnostics: (instance: string) => get<DiagnosticsView>(`/api/runs/${encodeURIComponent(instance)}/diagnostics`),
	summaries: (period: string, offset?: number, before?: string) => get<SummariesView>('/api/summaries', { period, offset, before }),
	correctSummary: (period: string, start: string, text: string) =>
		post<SummaryView>(`/api/summaries/${encodeURIComponent(period)}/${encodeURIComponent(start)}`, { text }),
	questions: () => get<QuestionsView>('/api/questions'),
	answer: (id: string, name: string, word: string) =>
		post<QuestionsView>(`/api/questions/${encodeURIComponent(id)}/answer`, { name, word: word.trim() || undefined }),
	skipQuestion: (id: string) => post<QuestionsView>(`/api/questions/${encodeURIComponent(id)}/skip`),
	settings: () => get<SettingsView>('/api/settings'),
	memory: () => get<MemoryView>('/api/memory'),
	list: (folder: string, q?: string, offset?: number) => get<ListView>(`/api/memory/${encodeURIComponent(folder)}`, { q, offset }),
	search: (q: string) => get<SearchView>('/api/search', { q }),
	graph: (id: string | undefined, depth: number, kinds?: string, categories?: string) =>
		get<GraphView>('/api/graph', { id, depth, kinds, categories }),
	editForm: (id: string) => get<EditView>(`/api/file/${encodeURIComponent(id)}/edit`),
	saveFile: (id: string, version: string, draft: Draft) =>
		post<FileView>(`/api/file/${encodeURIComponent(id)}/edit`, { version, ...draft }),
	deleteFile: (id: string, version: string) => post<SavedView>(`/api/file/${encodeURIComponent(id)}/delete`, { version }),
	createFile: (kind: string, draft: Draft, stem?: string) =>
		post<SavedView>('/api/files', {
			kind,
			keys: draft.keys,
			facts: draft.facts?.map((f) => f.text) ?? [],
			relations: draft.relations ?? [],
			notes: draft.notes || undefined,
			stem
		}),
	changeSettings: (language: string, time_zone: string) => post<SettingsView>('/api/settings', { language, time_zone })
};
