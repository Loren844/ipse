// The kinds of entity a memory holds, and the folder each lives in: the interface's
// addresses use the folder (`/memory/places/<id>`).
import { icons } from './icons.ts';

export type Kind = 'person' | 'place' | 'organization' | 'project' | 'work' | 'thing' | 'topic' | 'reminder';

export const KINDS: { kind: Kind; folder: string; icon: string }[] = [
	{ kind: 'person', folder: 'people', icon: icons.people },
	{ kind: 'place', folder: 'places', icon: icons.place },
	{ kind: 'organization', folder: 'organizations', icon: icons.organization },
	{ kind: 'project', folder: 'projects', icon: icons.project },
	{ kind: 'work', folder: 'works', icon: icons.work },
	{ kind: 'thing', folder: 'things', icon: icons.thing },
	{ kind: 'topic', folder: 'topics', icon: icons.topic },
	{ kind: 'reminder', folder: 'reminders', icon: icons.reminder }
];

/** The folder of a kind of entity; `null` for anything else (an event, a summary). */
export const folderOf = (kind: string | null | undefined) => KINDS.find((k) => k.kind === kind)?.folder ?? null;

/** The kind whose folder this is. */
export const kindOf = (folder: string) => KINDS.find((k) => k.folder === folder) ?? null;

/** The icon of a kind of file. */
export const iconOf = (kind: string | null | undefined) =>
	KINDS.find((k) => k.kind === kind)?.icon ?? (kind === 'event' ? icons.agenda : kind === 'summary' ? icons.summaries : icons.memory);
