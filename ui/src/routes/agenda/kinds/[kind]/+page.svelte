<script lang="ts">
	// One kind of event (issue #134): what is coming and what was, who and where most;
	// for trips, how long; for celebrations, the birthdays to come.
	import { page } from '$app/state';
	import { api, type EventView, type EventKindView } from '#lib/api.ts';
	import { addDays } from '#lib/calendar.ts';
	import Events from '#lib/Events.svelte';
	import { dayOf, isDay, longDay } from '#lib/format.ts';
	import { i18n } from '#lib/i18n.svelte.ts';
	import Mention from '#lib/Mention.svelte';
	import { Loader } from '#lib/remote.svelte.ts';

	const screen = new Loader<EventKindView | null>();
	$effect(() => {
		const kind = page.params.kind;
		if (kind) screen.load(() => api.kind(kind));
	});
	const view = $derived(screen.state.kind === 'ready' ? screen.state.value : null);
	const name = $derived(i18n.t.eventKinds[page.params.kind ?? ''] ?? page.params.kind ?? '');
	const today = dayOf(new Date().toISOString());

	/** How many days an event lasts: an all-day end is exclusive. */
	function days(event: EventView): number | null {
		if (!event.end) return null;
		const start = dayOf(event.start);
		const end = dayOf(event.end);
		let n = 0;
		for (let d = start; d < end && n < 1000; d = addDays(d, 1)) n += 1;
		return isDay(event.end) ? n : n + 1;
	}
</script>

<svelte:head><title>{name} · Ipse</title></svelte:head>

<div class="screen kind-page">
	<header class="page-head">
		<p class="crumbs"><a href="/agenda?kind={page.params.kind}">{i18n.t.agenda}</a></p>
		<h1>{name}</h1>
		{#if i18n.t.kindLeads[page.params.kind ?? '']}<p class="quiet">{i18n.t.kindLeads[page.params.kind ?? '']}</p>{/if}
		{#if view?.events}<p class="quiet">{i18n.t.eventKindCount(view.events)}</p>{/if}
	</header>

	{#snippet lasting(event: EventView)}
		{#if view?.kind === 'trip'}
			{@const n = days(event)}
			{#if n && n > 1}<span class="series">{i18n.t.lasting(n)}</span>{/if}
		{/if}
	{/snippet}

	{#if view}
		{#if view.kind === 'celebration'}
			<section aria-labelledby="birthdays">
				<h2 id="birthdays">{i18n.t.birthdaysTitle}</h2>
				{#if view.birthdays.length}
					<ol class="birthdays" data-testid="birthdays">
						{#each view.birthdays as b (b.person.id)}
							<li>
								<span class="when">{longDay(b.day, today)}</span>
								<Mention link={b.person} />
								{#if b.age !== null}<span class="quiet">{i18n.t.turns(b.age)}</span>{/if}
							</li>
						{/each}
					</ol>
				{:else}
					<p class="quiet">{i18n.t.birthdaysNone}</p>
				{/if}
			</section>
		{/if}

		{#if view.events === 0}
			<p class="quiet">{i18n.t.kindNone}</p>
		{:else}
			{#if view.people.length || view.places.length}
				<div class="kind-often">
					{#if view.people.length}
						<section aria-labelledby="with-whom">
							<h2 id="with-whom">{i18n.t.kindWithWhom}</h2>
							<ul class="often" data-testid="with-whom">
								{#each view.people as p, i (i)}
									<li><Mention link={p.link} /> <span class="quiet">{i18n.t.kindTimes(p.events)}</span></li>
								{/each}
							</ul>
						</section>
					{/if}
					{#if view.places.length}
						<section aria-labelledby="where">
							<h2 id="where">{i18n.t.kindWhere}</h2>
							<ul class="often" data-testid="where">
								{#each view.places as p, i (i)}
									<li><Mention link={p.link} /> <span class="quiet">{i18n.t.kindTimes(p.events)}</span></li>
								{/each}
							</ul>
						</section>
					{/if}
				</div>
			{/if}

			<section aria-labelledby="next">
				<h2 id="next">{i18n.t.kindNext}</h2>
				{#if view.next.length}
					<Events events={view.next} {today} detail={lasting} />
				{:else}
					<p class="quiet">{i18n.t.kindNothingNext}</p>
				{/if}
			</section>

			{#if view.last.length}
				<section aria-labelledby="last">
					<h2 id="last">{i18n.t.kindLast}</h2>
					<Events events={view.last} {today} detail={lasting} />
				</section>
			{/if}
		{/if}
	{:else if screen.state.kind === 'failed'}
		<p class="quiet">{i18n.t.failed}</p>
	{/if}
</div>
