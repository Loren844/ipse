// Dates and names as the person reads them, in the interface's language and the
// vault's time zone.
import { app } from './app.svelte.ts';
import { i18n } from './i18n.svelte.ts';

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

/** The page of a linked file. */
export function hrefOf(link: { id: string | null; kind: string | null }): string | null {
	if (!link.id) return null;
	return link.kind === 'event' ? `/agenda/${link.id}` : `/people/${link.id}`;
}
