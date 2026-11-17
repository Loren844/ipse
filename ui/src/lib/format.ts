import { folderOf } from './kinds.ts';
// Dates and names as the person reads them, in the interface's language and the
// vault's time zone.
import { app } from './app.svelte.ts';
import { i18n } from './i18n.svelte.ts';
import type { RunView } from './types/RunView.ts';

const locale = () => (i18n.language === 'fr' ? 'fr-FR' : 'en-GB');
const zone = () => (app.state.kind === 'ready' ? app.state.info.time_zone : undefined);

/** True for `YYYY-MM-DD`: a whole day, with no time. */
export const isDay = (text: string) => /^\d{4}-\d{2}-\d{2}$/.test(text);

/** The day an event starts, `YYYY-MM-DD`, in the vault's time zone. */
export function dayOf(text: string): string {
	if (isDay(text)) return text;
	const parts = new Intl.DateTimeFormat('en-CA', { timeZone: zone(), year: 'numeric', month: '2-digit', day: '2-digit' }).format(new Date(text));
	return parts;
}

/** `text` with a capital first letter, as at the start of a line. */
export const capitalize = (text: string) => text.charAt(0).toLocaleUpperCase() + text.slice(1);

/** "Samedi 21 novembre", with the year when it is not this year. */
export function longDay(day: string, today?: string): string {
	const date = new Date(`${day}T12:00:00Z`);
	const sameYear = today ? today.slice(0, 4) === day.slice(0, 4) : true;
	return capitalize(new Intl.DateTimeFormat(locale(), {
		timeZone: 'UTC',
		weekday: 'long',
		day: 'numeric',
		month: 'long',
		year: sameYear ? undefined : 'numeric'
	}).format(date));
}

/** "12 oct. 2025": a day in few words, with its year. */
export function shortDay(day: string): string {
	return new Intl.DateTimeFormat(locale(), { timeZone: 'UTC', day: 'numeric', month: 'short', year: 'numeric' }).format(new Date(`${day}T12:00:00Z`));
}

/** "Samedi 21 novembre, 19:30". */
export const when = (start: string, today?: string) => `${longDay(dayOf(start), today)}, ${timeOf(start)}`;

/** A French number in its usual groups, `+33 6 12 34 56 78`; others as written. */
export function phone(text: string): string {
	const french = /^\+33(\d)(\d{2})(\d{2})(\d{2})(\d{2})$/.exec(text.replace(/\s/g, ''));
	return french ? `+33 ${french.slice(1).join(' ')}` : text;
}

/** "19:30", or "Toute la journée" for a whole day. */
export function timeOf(text: string): string {
	if (isDay(text)) return i18n.t.allDay;
	return new Intl.DateTimeFormat(locale(), { timeZone: zone(), hour: '2-digit', minute: '2-digit' }).format(new Date(text));
}

/** "2 avril 1990, 36 ans", or "2 avril" without the year. */
export function birthday(text: string, today: Date = new Date()): string {
	const match = /^(\d{4}|-)-?(\d{2})-(\d{2})$/.exec(text.replace(/^--/, '-'));
	if (!match) return text;
	const [, year, month, day] = match;
	const date = new Date(Date.UTC(2000, Number(month) - 1, Number(day), 12));
	const dayMonth = new Intl.DateTimeFormat(locale(), { timeZone: 'UTC', day: 'numeric', month: 'long' }).format(date);
	if (year === '-') return dayMonth;
	let age = today.getFullYear() - Number(year);
	if (today.getMonth() + 1 < Number(month) || (today.getMonth() + 1 === Number(month) && today.getDate() < Number(day))) age -= 1;
	return `${dayMonth} ${year}, ${i18n.t.age(age)}`;
}

/** Up to two initials of a name. */
export function initials(name: string): string {
	const words = name.split(/\s+/).filter(Boolean);
	const letters = words.length > 1 ? [words[0], words[words.length - 1]] : words;
	return letters.map((w) => w[0]?.toLocaleUpperCase() ?? '').join('');
}

/** Where a file comes from, in words: "vos contacts et vous". */
export function sources(list: string[]): string {
	const names = list.map((s) => i18n.t.sources[s.replace(/-\d+$/, '')] ?? s);
	const unique = [...new Set(names)];
	return new Intl.ListFormat(locale(), { type: 'conjunction' }).format(unique);
}

/** "Vient de vos contacts.", "Vient d’une proposition acceptée.": where a file came from. */
export function fromSources(list: string[]): string {
	const text = sources(list);
	if (i18n.language === 'fr') return /^[aeiouyhéèêàâîïôû]/i.test(text) ? `Vient d’${text}.` : `Vient de ${text}.`;
	return `${i18n.t.from} ${text}.`;
}

/** The page of a linked file; summaries have none yet. */
export function hrefOf(link: { id: string | null; kind: string | null }): string | null {
	if (!link.id || link.kind === 'summary') return null;
	if (link.kind === 'event') return `/agenda/${link.id}`;
	return `/memory/${folderOf(link.kind) ?? 'people'}/${link.id}`;
}

/** "il y a 2 heures", "hier", or the day for anything older than a week. */
export function ago(text: string, now: Date = new Date()): string {
	const then = new Date(text);
	const seconds = Math.round((then.getTime() - now.getTime()) / 1000);
	const format = new Intl.RelativeTimeFormat(locale(), { numeric: 'auto' });
	const abs = Math.abs(seconds);
	if (abs < 60) return format.format(0, 'second');
	if (abs < 3600) return format.format(Math.round(seconds / 60), 'minute');
	if (abs < 86400) return format.format(Math.round(seconds / 3600), 'hour');
	if (abs < 7 * 86400) return format.format(Math.round(seconds / 86400), 'day');
	const today = dayOf(now.toISOString());
	return i18n.t.onDay(longDay(dayOf(text), today).replace(/^./, (c) => c.toLocaleLowerCase()));
}

/** Who made a proposal, in words: "Claude", "vos contacts", "Ipse". */
export function authorName(by: { kind: string; name: string }): string {
	return by.kind === 'source' ? sources([by.name]) : by.name;
}

/** A summary's period in words, from its name: "Samedi 14 novembre 2026", "Semaine 46
 * de 2026", "Novembre 2026", "2026". Any other text as it is. */
export function periodName(text: string): string {
	if (/^\d{4}-\d{2}-\d{2}$/.test(text)) return longDay(text, '0000');
	const week = /^(\d{4})-W(\d{2})$/.exec(text);
	if (week) return i18n.t.weekOf(Number(week[2]), week[1]);
	if (/^\d{4}-\d{2}$/.test(text)) {
		const month = new Intl.DateTimeFormat(locale(), { timeZone: 'UTC', month: 'long', year: 'numeric' }).format(new Date(`${text}-15T12:00:00Z`));
		return capitalize(month);
	}
	return text;
}

/** What an import or a sync brought: "12 fiches ajoutées, 3 mises à jour." */
export function runCounts(run: RunView): string {
	const t = i18n.t;
	const parts = [
		run.added && t.countAdded(run.added),
		run.updated && t.updated(run.updated),
		run.unchanged && t.unchanged(run.unchanged),
		run.cancelled && t.cancelledEvents(run.cancelled),
		run.removed && t.removedItems(run.removed),
		run.skipped && t.skipped(run.skipped),
		run.rejected && t.rejected(run.rejected),
		run.tasks && t.tasksMade(run.tasks)
	].filter(Boolean) as string[];
	return parts.length ? capitalize(parts.join(', ')) + '.' : t.nothingNew;
}
