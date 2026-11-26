<script lang="ts">
	import FirstVisit from '#lib/FirstVisit.svelte';
	import { onMount } from 'svelte';
	import { SvelteSet } from 'svelte/reactivity';
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { api, type EventView } from '#lib/api.ts';
	import { addDays, monthStart, monthWeeks, weekOf } from '#lib/calendar.ts';
	import CalendarMonth from '#lib/CalendarMonth.svelte';
	import CalendarWeek from '#lib/CalendarWeek.svelte';
	import { capitalize, dayOf, isDay, longDay, timeOf } from '#lib/format.ts';
	import { i18n } from '#lib/i18n.svelte.ts';
	import KindFilter from '#lib/KindFilter.svelte';
	import Mention from '#lib/Mention.svelte';
	import { rows, type Row } from '#lib/series.ts';
	import Timeline from '#lib/Timeline.svelte';

	// The timeline is the Years view (issue #211).
	type View = 'list' | 'week' | 'month' | 'years';
	const VIEWS: View[] = ['list', 'week', 'month', 'years'];

	let events: EventView[] = $state([]);
	let summaries: Record<string, string> = $state({});
	let from = $state('');
	let to = $state('');
	let ready = $state(false);
	let failed = $state(false);
	const today = dayOf(new Date().toISOString());

	// The week and month views: the period in the address, so that a reload or a link
	// keeps it.
	const view: View = $derived((VIEWS as string[]).includes(page.url.searchParams.get('view') ?? '') ? (page.url.searchParams.get('view') as View) : 'list');
	const day = $derived(isDay(page.url.searchParams.get('day') ?? '') ? (page.url.searchParams.get('day') as string) : today);
	// One kind of event at a time (issue #134), in the address too.
	const kind = $derived(page.url.searchParams.get('kind') || null);
	const ofKind = (list: EventView[]) => (kind ? list.filter((e) => e.event_kind === kind) : list);
	let period: { events: EventView[]; summaries: Record<string, string>; key: string } | null = $state(null);
	/** The kinds the events loaded have, to choose from. */
	const kinds = $derived.by(() => {
		const found = new Set<string>();
		for (const e of view === 'list' ? events : (period?.events ?? [])) if (e.event_kind) found.add(e.event_kind);
		if (kind) found.add(kind);
		return [...found];
	});
	let periodFailed = $state(false);

	const range = $derived.by(() => {
		if (view === 'week') {
			const days = weekOf(day);
			return { from: days[0], to: days[6] };
		}
		const weeks = monthWeeks(day);
		return { from: weeks[0][0], to: weeks[weeks.length - 1][6] };
	});
	const locale = $derived(i18n.language === 'fr' ? 'fr-FR' : 'en-GB');
	const title = $derived.by(() => {
		if (view === 'month') {
			return capitalize(new Intl.DateTimeFormat(locale, { timeZone: 'UTC', month: 'long', year: 'numeric' }).format(new Date(`${monthStart(day)}T12:00:00Z`)));
		}
		return i18n.t.weekFrom(longDay(weekOf(day)[0], today));
	});

	$effect(() => {
		if (view === 'list' || view === 'years') return;
		const key = `${range.from}/${range.to}`;
		if (period?.key === key) return;
		api
			.events(range.from, range.to)
			.then((answer) => {
				const found: Record<string, string> = {};
				for (const summary of answer.summaries) found[summary.day] = summary.text;
				period = { events: answer.events, summaries: found, key };
				periodFailed = false;
			})
			.catch(() => (periodFailed = true));
	});

	/** This screen with another view or day. */
	function show(next: { view?: View; day?: string; kind?: string | null }) {
		const url = new URL(page.url.href);
		const v = next.view ?? view;
		if (next.kind !== undefined) {
			if (next.kind) url.searchParams.set('kind', next.kind);
			else url.searchParams.delete('kind');
		}
		// The timeline's year, month and person (issue #244) stay with it.
		if (v !== 'years') {
			url.searchParams.delete('year');
			url.searchParams.delete('month');
			url.searchParams.delete('person');
		}
		if (v === 'list') {
			url.searchParams.delete('view');
			url.searchParams.delete('day');
		} else if (v === 'years') {
			url.searchParams.set('view', v);
			url.searchParams.delete('day');
		} else {
			url.searchParams.set('view', v);
			url.searchParams.set('day', next.day ?? day);
		}
		goto(`${url.pathname}${url.search}`, { reset: false, replace: true });
	}

	const step = (direction: 1 | -1) => show({ day: view === 'week' ? addDays(day, 7 * direction) : monthStart(day, direction) });

	const shift = (d: string, days: number) => addDays(d, days);

	async function load(start?: string, end?: string, where: 'replace' | 'before' | 'after' = 'replace') {
		try {
			const answer = await api.events(start, end);
			const fresh = answer.events.filter((e) => !events.some((x) => x.id === e.id));
			for (const summary of answer.summaries) summaries[summary.day] = summary.text;
			if (where === 'before') events = [...fresh, ...events];
			else if (where === 'after') events = [...events, ...fresh];
			else events = answer.events;
			if (where !== 'after') from = answer.from;
			if (where !== 'before') to = answer.to;
			ready = true;
		} catch {
			failed = true;
		}
	}

	onMount(() => load());

	// One thread for the whole period: each day opens with its name, then its events.
	// A repeating event shows once, at its next occurrence, and unfolds (issue #175).
	const open = new SvelteSet<string>();
	const days = $derived.by(() => {
		const groups: { day: string; events: Row[] }[] = [];
		for (const row of rows(ofKind(events), open, today)) {
			if (row.folded) continue;
			const d = dayOf(row.event.start);
			const last = groups[groups.length - 1];
			if (last?.day === d) last.events.push(row);
			else groups.push({ day: d, events: [row] });
		}
		return groups;
	});
</script>

<svelte:head><title>{view === 'years' ? i18n.t.timeline : i18n.t.agenda} · Ipse</title></svelte:head>

<div class="screen" class:wide={view === 'week' || view === 'month'}>
	<header class="page-head head-row">
		<h1>{i18n.t.agenda}</h1>
		<a class="button small" href="/memory/new?kind=event">+ {i18n.t.addKind.event}</a>
	</header>
	<FirstVisit screen="agenda" />
	<div class="agenda-bar">
		<div class="segmented" role="group" aria-label={i18n.t.agendaViews}>
			{#each VIEWS as v (v)}
				<button type="button" aria-pressed={view === v} onclick={() => show({ view: v })}>{i18n.t.agendaView[v]}</button>
			{/each}
		</div>
		{#if view === 'week' || view === 'month'}
			<div class="period-nav">
				<button type="button" class="button small" aria-label={view === 'week' ? i18n.t.previousWeek : i18n.t.previousMonth} onclick={() => step(-1)}
					>‹</button
				>
				<button type="button" class="button small" onclick={() => show({ day: today })}>{i18n.t.today}</button>
				<button type="button" class="button small" aria-label={view === 'week' ? i18n.t.nextWeek : i18n.t.nextMonth} onclick={() => step(1)}>›</button>
			</div>
		{/if}
	</div>

	{#if kinds.length && view !== 'years'}
		<div class="kind-bar">
			<KindFilter {kinds} bind:kind={() => kind, (k) => show({ kind: k })} />
			{#if kind}
				<a class="link-button" href="/agenda/kinds/{kind}">{i18n.t.kindPageLink(i18n.t.eventKinds[kind] ?? kind)}</a>
			{/if}
		</div>
	{/if}

	{#if view === 'years'}
		<Timeline />
	{:else if view !== 'list'}
		<h2 class="period" aria-live="polite">{title}</h2>
		{#if view === 'month'}
			<p class="period-link">
				<a href="/agenda?view=years&year={monthStart(day).slice(0, 4)}&month={monthStart(day).slice(5, 7)}">{i18n.t.monthInTimeline}</a>
			</p>
		{/if}
		{#if period}
			{#key `${view} ${day} ${kind}`}
				<div class="appear">
					{#if view === 'week'}
						<CalendarWeek {day} events={ofKind(period.events)} summaries={period.summaries} {today} />
					{:else}
						<CalendarMonth {day} events={ofKind(period.events)} summaries={period.summaries} {today} {title} />
					{/if}
				</div>
			{/key}
		{:else if periodFailed}
			<p class="quiet">{i18n.t.failed}</p>
		{/if}
	{:else if ready}
		<button type="button" class="more" onclick={() => load(shift(from, -90), shift(from, -1), 'before')}>{i18n.t.earlier}</button>
		{#if days.length}
			<ol class="thread agenda appear">
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
					{#each group.events as { event, count } (event.id)}
						<li class="knot" class:past={group.day < today} class:cancelled={event.status === 'cancelled'}>
							<span class="when">
								{timeOf(event.start)}
								{#if event.status === 'cancelled'}<span class="badge">{i18n.t.cancelled}</span>{/if}
								{#if event.event_kind}<a class="kind-tag" href="/agenda/kinds/{event.event_kind}"
										>{i18n.t.eventKinds[event.event_kind] ?? event.event_kind}</a
									>{/if}
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
