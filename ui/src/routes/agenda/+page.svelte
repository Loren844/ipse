<script lang="ts">
	import { onMount } from 'svelte';
	import { api, type EventView } from '#lib/api.ts';
	import { dayOf, longDay, timeOf } from '#lib/format.ts';
	import { i18n } from '#lib/i18n.svelte.ts';
	import Mention from '#lib/Mention.svelte';

	let events: EventView[] = $state([]);
	let summaries: Record<string, string> = $state({});
	let from = $state('');
	let to = $state('');
	let ready = $state(false);
	let failed = $state(false);
	const today = dayOf(new Date().toISOString());

	const shift = (day: string, days: number) => {
		const date = new Date(`${day}T12:00:00Z`);
		date.setUTCDate(date.getUTCDate() + days);
		return date.toISOString().slice(0, 10);
	};

	async function load(start?: string, end?: string, where: 'replace' | 'before' | 'after' = 'replace') {
		try {
			const view = await api.events(start, end);
			const fresh = view.events.filter((e) => !events.some((x) => x.id === e.id));
			for (const summary of view.summaries) summaries[summary.day] = summary.text;
			if (where === 'before') events = [...fresh, ...events];
			else if (where === 'after') events = [...events, ...fresh];
			else events = view.events;
			if (where !== 'after') from = view.from;
			if (where !== 'before') to = view.to;
			ready = true;
		} catch {
			failed = true;
		}
	}

	onMount(() => load());

	// One thread for the whole period: each day opens with its name, then its events.
	const days = $derived.by(() => {
		const groups: { day: string; events: EventView[] }[] = [];
		for (const event of events) {
			const day = dayOf(event.start);
			const last = groups[groups.length - 1];
			if (last?.day === day) last.events.push(event);
			else groups.push({ day, events: [event] });
		}
		return groups;
	});
</script>

<div class="screen">
	<header class="page-head"><h1>{i18n.t.agenda}</h1></header>
	{#if ready}
		<button type="button" class="more" onclick={() => load(shift(from, -90), shift(from, -1), 'before')}>{i18n.t.earlier}</button>
		{#if days.length}
			<ol class="thread agenda">
				{#each days as group (group.day)}
					<li class="day-label" class:today={group.day === today}>
						<h2>{group.day === today ? i18n.t.today : longDay(group.day, today)}</h2>
						{#if summaries[group.day]}
							<p class="day-summary">
								{summaries[group.day]}
								<a href="/summaries?before={group.day}#day-{group.day}" class="visually-hidden">{i18n.t.readSummary}</a>
							</p>
						{/if}
					</li>
					{#each group.events as event (event.id)}
						<li class="knot" class:past={group.day < today} class:cancelled={event.status === 'cancelled'}>
							<span class="when">
								{timeOf(event.start)}
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
				{/each}
			</ol>
		{:else}
			<p class="quiet">{i18n.t.noEventsPeriod}</p>
		{/if}
		<button type="button" class="more" onclick={() => load(shift(to, 1), shift(to, 90), 'after')}>{i18n.t.later}</button>
	{:else if failed}
		<p class="quiet">{i18n.t.failed}</p>
	{/if}
</div>
