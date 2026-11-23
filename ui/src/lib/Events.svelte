<script lang="ts">
	// Events on the thread: one knot each, by start; past ones fainter. With `group`, a
	// repeating event is shown once, at its first occurrence here, and unfolds.
	import { SvelteSet } from 'svelte/reactivity';
	import type { EventView } from './api.ts';
	import { dayOf, timeOf, when } from './format.ts';
	import { i18n } from './i18n.svelte.ts';
	import Mention from './Mention.svelte';
	import { rows } from './series.ts';

	let { events, today, showDay = true, group = false }: { events: EventView[]; today: string; showDay?: boolean; group?: boolean } = $props();

	const open = new SvelteSet<string>();
	const shown = $derived(group ? rows(events, open, today) : events.map((event) => ({ event, count: 1, folded: false })));
</script>

<ol class="thread">
	{#each shown as { event, count, folded } (event.id)}
		{#if !folded}
			{@const day = dayOf(event.start)}
			<li class="knot" class:past={day < today} class:cancelled={event.status === 'cancelled'}>
				<span class="when">
					{showDay ? when(event.start, today) : timeOf(event.start)}
					{#if event.status === 'cancelled'}<span class="badge">{i18n.t.cancelled}</span>{/if}
					{#if event.event_kind}<span class="kind-tag">{i18n.t.eventKinds[event.event_kind] ?? event.event_kind}</span>{/if}
				</span>
				<a class="title" href="/agenda/{event.id}">{event.title}</a>
				{#if count > 1 && event.series}
					{@const key = event.series.key}
					<span class="series">
						{i18n.t.every(event.series.weekday, event.series.time)}
						<button type="button" class="unfold" aria-expanded={open.has(key)} onclick={() => (open.has(key) ? open.delete(key) : open.add(key))}>
							{open.has(key) ? i18n.t.hideSeries : i18n.t.showSeries(count)}
						</button>
					</span>
				{/if}
				{#if event.people.length || event.place}
					<span class="mentions">
						{#each event.people as person, i (i)}<Mention link={person} />{/each}
						{#if event.place}<span class="place">{event.place.name}</span>{/if}
					</span>
				{/if}
			</li>
		{/if}
	{/each}
</ol>
