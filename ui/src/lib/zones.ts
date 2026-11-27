// Time zones as a person reads them (issue #260): a city and the zone's name in the
// interface's language, this computer's zone first, never `Etc/UTC` or `Africa/Asmera`.

export type Zone = {
	/** The IANA name, saved in the memory's settings. */
	id: string;
	/** What the list shows: "Toronto · heure de l’Est (UTC−5)". */
	label: string;
	/** The label and the id, lower case and without accents, to search in. */
	search: string;
};

/** Every time zone the browser knows; empty when it cannot list them. */
function known(): string[] {
	try {
		return Intl.supportedValuesOf('timeZone');
	} catch {
		return [];
	}
}

/** This computer's zone, as the browser says; `null` when it does not. */
export function systemZone(): string | null {
	try {
		return Intl.DateTimeFormat().resolvedOptions().timeZone || null;
	} catch {
		return null;
	}
}

/** Lower case, without accents: "Zürich" is found by "zurich". */
export function fold(text: string): string {
	return text
		.normalize('NFD')
		.replace(/\p{M}/gu, '')
		.toLocaleLowerCase();
}

/** One part of how `locale` names `id` at `at`, or `''`. */
function part(locale: string, id: string, style: 'longGeneric' | 'shortOffset', at: Date): string {
	try {
		const parts = new Intl.DateTimeFormat(locale, { timeZone: id, timeZoneName: style }).formatToParts(at);
		return parts.find((p) => p.type === 'timeZoneName')?.value ?? '';
	} catch {
		return '';
	}
}

/** How the list names `id`, in `locale`. */
export function zoneLabel(id: string, locale: string, at: Date = new Date()): string {
	if (id === 'UTC' || id === 'Etc/UTC') return 'UTC';
	const city = (id.split('/').pop() ?? id).replaceAll('_', ' ');
	const name = part(locale, id, 'longGeneric', at);
	const offset = part(locale, id, 'shortOffset', at);
	const described = name && name !== offset ? `${city} · ${name}` : city;
	return offset ? `${described} (${offset})` : described;
}

/** The zones to choose from: this computer's first, then the memory's, then the others
 * by name. Administrative ones (`Etc/…`) are left out, except UTC. */
export function zoneList(locale: string, system: string | null, memory: string): Zone[] {
	const at = new Date();
	const ids = known().filter((id) => id.includes('/') && !id.startsWith('Etc/'));
	const first = [system, memory].filter((id, i, all): id is string => !!id && all.indexOf(id) === i);
	const rest = [...new Set([...ids, 'UTC'])].filter((id) => !first.includes(id));
	const make = (id: string): Zone => {
		const label = zoneLabel(id, locale, at);
		return { id, label, search: fold(`${label} ${id.replaceAll('_', ' ')}`) };
	};
	const collator = new Intl.Collator(locale);
	return [...first.map(make), ...rest.map(make).sort((a, b) => collator.compare(a.label, b.label))];
}
