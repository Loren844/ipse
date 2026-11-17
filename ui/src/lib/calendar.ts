// Days for the agenda's week and month views (issue #132): plain `YYYY-MM-DD` strings,
// weeks from Monday, as in French and British calendars.
import { dayOf, isDay } from './format.ts';
import type { EventView } from './types/EventView.ts';

/** `day` moved by `days`. */
export function addDays(day: string, days: number): string {
	const date = new Date(`${day}T12:00:00Z`);
	date.setUTCDate(date.getUTCDate() + days);
	return date.toISOString().slice(0, 10);
}

/** The Monday of `day`'s week. */
export function mondayOf(day: string): string {
	const weekday = (new Date(`${day}T12:00:00Z`).getUTCDay() + 6) % 7;
	return addDays(day, -weekday);
}

/** The seven days of `day`'s week. */
export function weekOf(day: string): string[] {
	const monday = mondayOf(day);
	return Array.from({ length: 7 }, (_, i) => addDays(monday, i));
}

/** The first day of `day`'s month, moved by `months`. */
export function monthStart(day: string, months = 0): string {
	const date = new Date(`${day.slice(0, 7)}-01T12:00:00Z`);
	date.setUTCMonth(date.getUTCMonth() + months);
	return date.toISOString().slice(0, 10);
}

/** The weeks shown for `day`'s month: whole weeks, Monday to Sunday. */
export function monthWeeks(day: string): string[][] {
	const first = monthStart(day);
	const last = addDays(monthStart(day, 1), -1);
	const weeks: string[][] = [];
	for (let monday = mondayOf(first); monday <= last; monday = addDays(monday, 7)) weeks.push(weekOf(monday));
	return weeks;
}

/** The days an event covers, up to a month: a trip shows on each of its days. */
export function daysOf(event: EventView): string[] {
	const first = dayOf(event.start);
	if (!event.end) return [first];
	// An end is exclusive for a whole day (`2026-03-06` to `2026-03-08` is two days)
	// and for a time at midnight.
	let last = dayOf(event.end);
	if (isDay(event.end) || event.end.slice(11, 16) === '00:00') last = addDays(last, -1);
	if (last <= first) return [first];
	const days: string[] = [];
	for (let day = first; day <= last && days.length < 31; day = addDays(day, 1)) days.push(day);
	return days;
}

/** Each day's events, all-day ones first, then by start. */
export function byDay(events: EventView[]): Map<string, EventView[]> {
	const days = new Map<string, EventView[]>();
	for (const event of events) {
		for (const day of daysOf(event)) {
			const list = days.get(day) ?? [];
			list.push(event);
			days.set(day, list);
		}
	}
	for (const list of days.values()) {
		list.sort((a, b) => Number(!isDay(a.start)) - Number(!isDay(b.start)) || a.start.localeCompare(b.start));
	}
	return days;
}
