// What a proposal changes, in plain words: its title in the inbox and on its own page,
// and each change as the diff shows it. Never fact ids or front matter.
import type { ChangeView, FileChangesView, ProposalLine } from './api.ts';
import { birthday, isDay, periodName, phone, when } from './format.ts';
import { i18n } from './i18n.svelte.ts';

/** Keys holding a list: only the items that come and go are shown. */
const LISTS = new Set(['phones', 'emails', 'aliases', 'participants']);

/** Names listed in a title, at most; the rest are counted. */
const NAMED = 3;

/** What a change is about: a key's name, "Fait", "Relation"… */
export function label(change: ChangeView): string {
	const t = i18n.t;
	if (change.what === 'key') return t.keys[change.key ?? ''] ?? change.key ?? '';
	if (change.what === 'fact') return t.fact;
	if (change.what === 'relation') return t.relation;
	if (change.what === 'summary') return t.summaryLabel;
	if (change.what === 'merged') return t.mergedLabel;
	if (change.what === 'links') return t.linksLabel;
	return t.notesLabel;
}

/** A key's value as the file's own page shows it. */
export function shown(change: ChangeView, text: string): string {
	if (change.what !== 'key') return text;
	if (change.key === 'phones') return text.split(', ').map(phone).join(', ');
	if (change.key === 'birthday') return birthday(text);
	if (change.key === 'kind') return i18n.t.eventKinds[text] ?? text;
	if ((change.key === 'start' || change.key === 'end') && !isDay(text) && !Number.isNaN(Date.parse(text))) return when(text);
	return text;
}

export type Sides = { removed: string[]; added: string[] };

/** What a change removes and adds; for a list, only the items that come and go. */
export function sides(change: ChangeView): Sides {
	if (change.what === 'merged' || change.what === 'links') return { removed: [], added: [] };
	const before = change.before === null ? [] : [shown(change, change.before)];
	const after = change.after === null ? [] : [shown(change, change.after)];
	if (change.what !== 'key' || !LISTS.has(change.key ?? '')) return { removed: before, added: after };
	const items = (text: string | null) => (text === null ? [] : text.split(', ').map((t) => shown(change, t)));
	const was = items(change.before);
	const is = items(change.after);
	return { removed: was.filter((t) => !is.includes(t)), added: is.filter((t) => !was.includes(t)) };
}

/** A file's name as the diff heads it: "Résumé : samedi 14 novembre" for a summary. */
export function fileName(file: FileChangesView): string {
	return file.kind === 'summary' ? i18n.t.summaryOf(periodName(file.name)) : file.name;
}

/** The only change a proposal makes, when it makes just one: shown in the inbox. */
export function single(line: ProposalLine): FileChangesView | null {
	return line.files.length === 1 && line.files[0].changes.length === 1 ? line.files[0] : null;
}

/** A proposal's title: what it changes, else its reason when nothing says so. */
export function title(line: ProposalLine): string {
	return titleOf(line.files) ?? line.reason;
}

/** The files' names, each once, the first few: "Yoga, Studio Lotus et 7 autres". */
function names(files: FileChangesView[]): string {
	const all = [...new Set(files.map(fileName))];
	const shownNames = all.length > NAMED ? all.slice(0, NAMED - 1) : all;
	if (all.length > NAMED) shownNames.push(i18n.t.others(all.length - shownNames.length));
	return new Intl.ListFormat(i18n.language === 'fr' ? 'fr-FR' : 'en-GB', { type: 'conjunction' }).format(shownNames);
}

/** "Marc Leroy : joue au tennis le jeudi", "Fusionner Lily S. dans Lily Smith"… */
export function titleOf(files: FileChangesView[]): string | null {
	const t = i18n.t;
	if (!files.length) return null;
	const merged = files.flatMap((f) => f.changes).find((c) => c.what === 'merged');
	if (merged) return t.titleMerge(merged.before ?? '', merged.after ?? files[0].name);
	if (files.every((f) => f.kind === 'summary')) return files.length === 1 ? fileName(files[0]) : t.titleSummaries(files.length, names(files));
	if (files.every((f) => f.created)) return files.length === 1 ? t.titleNew(files[0].name) : t.titleNewFiles(files.length, names(files));
	const kinds = eventKinds(files);
	if (kinds) return kinds;
	if (files.length > 1) return t.titleFiles(files.length, names(files));
	const [file] = files;
	if (file.changes.length !== 1) return t.about(file.name, t.changes(file.changes.length));
	return one(file, file.changes[0]);
}

/** Many events given a kind at once: "30 événements reçoivent un type : Repas (18), Sport (7)". */
function eventKinds(files: FileChangesView[]): string | null {
	if (files.length < 2) return null;
	const counts = new Map<string, number>();
	for (const file of files) {
		const [change] = file.changes;
		if (file.kind !== 'event' || file.changes.length !== 1 || change.what !== 'key' || change.key !== 'kind') return null;
		const kind = change.after === null ? i18n.t.eventKindNone : (i18n.t.eventKinds[change.after] ?? change.after);
		counts.set(kind, (counts.get(kind) ?? 0) + 1);
	}
	const parts = [...counts].sort((a, b) => b[1] - a[1]).map(([kind, n]) => `${kind} (${n})`);
	return i18n.t.titleKinds(files.length, parts.join(', '));
}

function one(file: FileChangesView, change: ChangeView): string {
	const t = i18n.t;
	const { removed, added } = sides(change);
	if (change.what === 'key') {
		if (added.length) return t.titleKey(file.name, label(change), added.join(', '));
		return t.titleKeyRemoved(file.name, label(change), removed.join(', '));
	}
	if (change.what === 'fact' || change.what === 'relation') {
		if (added.length) return t.about(file.name, added.join(', '));
		return t.titleRemoved(file.name, removed.join(', '));
	}
	return t.about(file.name, label(change));
}
