<script lang="ts">
	// Events on the thread: one knot each, by start; past ones fainter.
	import type { EventView } from './api.ts';
	import { dayOf, timeOf, when } from './format.ts';
	import { i18n } from './i18n.svelte.ts';
	import Mention from './Mention.svelte';

	let { events, today, showDay = true }: { events: EventView[]; today: string; showDay?: boolean } = $props();
</script>

<ol class="thread">
	{#each events as event (event.id)}
		{@const day = dayOf(event.start)}
		<li class="knot" class:past={day < today} class:cancelled={event.status === 'cancelled'}>
			<span class="when">
				{showDay ? when(event.start, today) : timeOf(event.start)}
				{#if event.status === 'cancelled'}<span class="badge">{i18n.t.cancelled}</span>{/if}
			</span>
			<a class="title" href="/agenda/{event.id}">{event.title}</a>
			{#if event.people.length || event.place}
				<span class="mentions">
					{#each event.people as person, i (i)}<Mention link={person} />{/each}
					{#if event.place}<span class="place">{event.place.name}</span>{/if}
				</span>
			{/if}
		</li>
	{/each}
</ol>
