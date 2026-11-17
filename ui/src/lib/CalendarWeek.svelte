<script lang="ts">
	// A week of the agenda (issue #132): seven days side by side, or one under the other
	// on a phone. Each day opens its summary when it has one.
	import type { EventView } from './api.ts';
	import { byDay, weekOf } from './calendar.ts';
	import { longDay, timeOf } from './format.ts';
	import { i18n } from './i18n.svelte.ts';

	let { day, events, summaries, today }: { day: string; events: EventView[]; summaries: Record<string, string>; today: string } =
		$props();

	const days = $derived(weekOf(day));
	const shown = $derived(byDay(events));
</script>

<ol class="week" aria-label={i18n.t.weekFrom(longDay(days[0], today))}>
	{#each days as d (d)}
		{@const list = shown.get(d) ?? []}
		<li class="week-day" class:today={d === today} aria-current={d === today ? 'date' : undefined}>
			<h3>
				<span class="weekday">{new Intl.DateTimeFormat(i18n.language === 'fr' ? 'fr-FR' : 'en-GB', { timeZone: 'UTC', weekday: 'short' }).format(new Date(`${d}T12:00:00Z`))}</span>
				<span class="date-number">{Number(d.slice(8))}</span>
				<span class="visually-hidden">{longDay(d, today)}</span>
			</h3>
			{#if summaries[d]}
				<a class="day-summary-link" href="/summaries?before={d}#day-{d}">{i18n.t.readSummary}</a>
			{/if}
			{#if list.length}
				<ul class="week-events">
					{#each list as event (event.id)}
						<li class:cancelled={event.status === 'cancelled'}>
							<span class="when">{timeOf(event.start)}</span>
							<a href="/agenda/{event.id}">{event.title}</a>
							{#if event.event_kind}<span class="kind-tag">{i18n.t.eventKinds[event.event_kind] ?? event.event_kind}</span>{/if}
							{#if event.status === 'cancelled'}<span class="badge">{i18n.t.cancelled}</span>{/if}
						</li>
					{/each}
				</ul>
			{:else}
				<p class="nothing">{i18n.t.nothingThatDay}</p>
			{/if}
		</li>
	{/each}
</ol>
