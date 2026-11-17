<script lang="ts">
	// A month of the agenda (issue #132): a grid of weeks, from Monday. A day shows its
	// first events; its number opens its week. On a phone, only how many there are.
	import type { EventView } from './api.ts';
	import { byDay, monthWeeks } from './calendar.ts';
	import { isDay, longDay, timeOf } from './format.ts';
	import { i18n } from './i18n.svelte.ts';
	import { icons } from './icons.ts';

	let {
		day,
		events,
		summaries,
		today,
		title
	}: { day: string; events: EventView[]; summaries: Record<string, string>; today: string; title: string } = $props();

	/** Events shown in a day before "+ n". */
	const SHOWN = 3;

	const weeks = $derived(monthWeeks(day));
	const shown = $derived(byDay(events));
	const locale = $derived(i18n.language === 'fr' ? 'fr-FR' : 'en-GB');
	const weekdays = $derived(
		(weeks[0] ?? []).map((d) => {
			const date = new Date(`${d}T12:00:00Z`);
			return {
				short: new Intl.DateTimeFormat(locale, { timeZone: 'UTC', weekday: 'short' }).format(date),
				long: new Intl.DateTimeFormat(locale, { timeZone: 'UTC', weekday: 'long' }).format(date)
			};
		})
	);
</script>

<table class="month">
	<caption class="visually-hidden">{title}</caption>
	<thead>
		<tr>
			{#each weekdays as weekday (weekday.long)}<th scope="col"><abbr title={weekday.long}>{weekday.short}</abbr></th>{/each}
		</tr>
	</thead>
	<tbody>
		{#each weeks as week (week[0])}
			<tr>
				{#each week as d (d)}
					{@const list = shown.get(d) ?? []}
					<td class:other={d.slice(0, 7) !== day.slice(0, 7)} class:today={d === today} aria-current={d === today ? 'date' : undefined}>
						<div class="cell-head">
							<a class="day-number" href="/agenda?view=week&day={d}" aria-label={longDay(d, today)}>{Number(d.slice(8))}</a>
							{#if summaries[d]}
								<a class="cell-summary" href="/summaries?before={d}#day-{d}" title={i18n.t.readSummary} aria-label="{i18n.t.readSummary}, {longDay(d, today)}"
									><svg viewBox="0 0 24 24" width="14" height="14" aria-hidden="true"><path d={icons.summaries} /></svg></a
								>
							{/if}
						</div>
						{#if list.length}
							<ul class="cell-events">
								{#each list.slice(0, SHOWN) as event (event.id)}
									<li class:cancelled={event.status === 'cancelled'}>
										<a href="/agenda/{event.id}">
											{#if !isDay(event.start)}<span class="when">{timeOf(event.start)}</span>{/if}
											{event.title}{#if event.event_kind}<span class="visually-hidden">, {i18n.t.eventKinds[event.event_kind] ?? event.event_kind}</span>{/if}
										</a>
									</li>
								{/each}
							</ul>
							{#if list.length > SHOWN}
								<a class="cell-more" href="/agenda?view=week&day={d}">{i18n.t.moreEvents(list.length - SHOWN)}</a>
							{/if}
							<span class="cell-count" aria-hidden="true">{list.length}</span>
						{/if}
					</td>
				{/each}
			</tr>
		{/each}
	</tbody>
</table>
