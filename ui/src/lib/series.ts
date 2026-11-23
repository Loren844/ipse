// Repeating events, shown once in a list (issue #175): one occurrence stands for its
// series (the next one, else the latest), the others are folded until unfolded.
import type { EventView } from './api.ts';
import { dayOf } from './format.ts';

export type Row = {
	event: EventView;
	/** For the occurrence standing for its series: how many the list holds. */
	count: number;
	/** An occurrence folded under the one standing for its series. */
	folded: boolean;
};

/** The rows of `events`, in their order; the series in `open` are not folded. */
export function rows(events: EventView[], open: ReadonlySet<string>, today: string): Row[] {
	const counts = new Map<string, number>();
	const leader = new Map<string, EventView>();
	for (const event of events) {
		const key = event.series?.key;
		if (!key) continue;
		counts.set(key, (counts.get(key) ?? 0) + 1);
		const current = leader.get(key);
		// The first one from today on; until there is one, the latest before.
		if (!current || (dayOf(current.start) < today && (dayOf(event.start) >= today || event.start > current.start))) {
			leader.set(key, event);
		}
	}
	return events.map((event) => {
		const key = event.series?.key;
		const count = key ? (counts.get(key) ?? 1) : 1;
		if (!key || count < 2) return { event, count: 1, folded: false };
		if (leader.get(key)?.id === event.id) return { event, count, folded: false };
		return { event, count: 1, folded: !open.has(key) };
	});
}
