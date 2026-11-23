<script lang="ts">
	// The timeline (issue #133): a life by years, then months. A year's strip shows how
	// full each month was; a month opens what marked it, then its calendar.
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { api, type MonthView, type TimelineView, type YearView } from '#lib/api.ts';
	import { capitalize, dayOf, longDay } from '#lib/format.ts';
	import { i18n } from '#lib/i18n.svelte.ts';
	import Mention from '#lib/Mention.svelte';
	import { Loader } from '#lib/remote.svelte.ts';

	const remote = new Loader<TimelineView>();
	remote.load(() => api.timeline());
	const view = $derived(remote.state.kind === 'ready' ? remote.state.value : null);

	const year = $derived(Number(page.url.searchParams.get('year')) || null);
	const month = $derived(page.url.searchParams.get('month'));
	const chosenYear = $derived(view?.years.find((y) => y.year === year) ?? null);
	const chosenMonth = $derived(chosenYear?.months.find((m) => m.month === `${year}-${month}`) ?? null);
	const today = dayOf(new Date().toISOString());
	const locale = $derived(i18n.language === 'fr' ? 'fr-FR' : 'en-GB');

	const monthName = (key: string, style: 'long' | 'short' = 'long') =>
		capitalize(new Intl.DateTimeFormat(locale, { timeZone: 'UTC', month: style }).format(new Date(`${key}-01T12:00:00Z`)));
	const monthTitle = (key: string) =>
		capitalize(new Intl.DateTimeFormat(locale, { timeZone: 'UTC', month: 'long', year: 'numeric' }).format(new Date(`${key}-01T12:00:00Z`)));

	/** The twelve months of a year, empty ones included, for its strip. */
	function strip(y: YearView): { key: string; month: MonthView | null }[] {
		return Array.from({ length: 12 }, (_, i) => {
			const key = `${y.year}-${String(i + 1).padStart(2, '0')}`;
			return { key, month: y.months.find((m) => m.month === key) ?? null };
		});
	}

	const fullest = $derived(Math.max(1, ...(view?.years.flatMap((y) => y.months.map((m) => m.events)) ?? [1])));

	function open(next: { year?: number | null; month?: string | null }) {
		const url = new URL(page.url.href);
		url.search = '';
		if (next.year) url.searchParams.set('year', String(next.year));
		if (next.year && next.month) url.searchParams.set('month', next.month.slice(5));
		goto(`${url.pathname}${url.search}`, { reset: false });
	}
</script>

<svelte:head><title>{i18n.t.timeline} · Ipse</title></svelte:head>

{#snippet people(list: YearView['people'])}
	{#if list.length}
		<p class="seen">
			<span class="quiet">{i18n.t.seenMost}</span>
			{#each list as seen, i (i)}<Mention link={seen.person} /><span class="seen-count">{seen.events}</span>{/each}
		</p>
	{/if}
{/snippet}

{#snippet monthStrip(y: YearView)}
	<ol class="strip" aria-label={i18n.t.monthsOf(y.year)}>
		{#each strip(y) as cell (cell.key)}
			<li>
				{#if cell.month}
					<a
						href="/timeline?year={y.year}&month={cell.key.slice(5)}"
						aria-label="{monthTitle(cell.key)}, {i18n.t.eventsCount(cell.month.events)}"
						onclick={(e) => {
							e.preventDefault();
							open({ year: y.year, month: cell.key });
						}}
					>
						<span class="bar" style="--fill: {Math.max(0.08, cell.month.events / fullest)}"></span>
						<span class="strip-label" aria-hidden="true">{monthName(cell.key, 'short')}</span>
					</a>
				{:else}
					<span class="bar empty" aria-hidden="true"></span>
					<span class="strip-label" aria-hidden="true">{monthName(cell.key, 'short')}</span>
				{/if}
			</li>
		{/each}
	</ol>
{/snippet}

<div class="screen timeline-screen">
	<header class="page-head">
		<nav class="crumbs" aria-label={i18n.t.whereInTimeline}>
			{#if year}
				<a href="/timeline" onclick={(e) => (e.preventDefault(), open({}))}>{i18n.t.timeline}</a>
				{#if chosenMonth}
					<span aria-hidden="true">›</span>
					<a href="/timeline?year={year}" onclick={(e) => (e.preventDefault(), open({ year }))}>{year}</a>
				{/if}
			{/if}
		</nav>
		<h1>{chosenMonth ? monthTitle(chosenMonth.month) : chosenYear ? chosenYear.year : i18n.t.timeline}</h1>
		{#if !year}<p class="intro">{i18n.t.timelineLead}</p>{/if}
	</header>

	{#if remote.state.kind === 'failed'}
		<p class="quiet">{i18n.t.failed}</p>
	{:else if view && !view.years.length}
		<p class="quiet">{i18n.t.timelineEmpty}</p>
	{:else if chosenMonth}
		<section class="period-detail" aria-label={monthTitle(chosenMonth.month)}>
			<p class="period-count">{i18n.t.eventsCount(chosenMonth.events)}</p>
			{#if chosenMonth.summary}<p class="period-summary">{chosenMonth.summary}</p>{/if}
			{#if chosenMonth.marks.length}
				<h2>{i18n.t.whatMarked}</h2>
				<ul class="marks">
					{#each chosenMonth.marks as mark (mark.id)}
						<li>
							<span class="kind-tag">{i18n.t.eventKinds[mark.kind] ?? mark.kind}</span>
							<a href="/agenda/{mark.id}">{mark.title}</a>
							<span class="quiet">{longDay(dayOf(mark.start), today)}</span>
						</li>
					{/each}
				</ul>
			{/if}
			{@render people(chosenMonth.people)}
			<p><a class="button" href="/agenda?view=month&day={chosenMonth.month}-01">{i18n.t.monthInAgenda}</a></p>
		</section>
	{:else if chosenYear}
		<section class="period-detail" aria-label={String(chosenYear.year)}>
			<p class="period-count">{i18n.t.eventsCount(chosenYear.events)}</p>
			{#if chosenYear.summary}<p class="period-summary">{chosenYear.summary}</p>{/if}
			{@render people(chosenYear.people)}
			{@render monthStrip(chosenYear)}
			<ol class="months">
				{#each chosenYear.months as m (m.month)}
					<li class="month-card">
						<h2>
							<a href="/timeline?year={chosenYear.year}&month={m.month.slice(5)}" onclick={(e) => (e.preventDefault(), open({ year: chosenYear.year, month: m.month }))}
								>{monthName(m.month)}</a
							>
						</h2>
						<p class="quiet">{i18n.t.eventsCount(m.events)}</p>
						{#if m.summary}<p class="month-summary">{m.summary}</p>{/if}
						{#if m.marks.length}
							<p class="month-marks">
								{#each m.marks as mark (mark.id)}<a href="/agenda/{mark.id}">{mark.title}</a>{/each}
							</p>
						{/if}
						{@render people(m.people)}
					</li>
				{/each}
			</ol>
		</section>
	{:else if view}
		<ol class="years">
			{#each view.years as y (y.year)}
				<li class="year-row">
					<h2>
						<a href="/timeline?year={y.year}" onclick={(e) => (e.preventDefault(), open({ year: y.year }))}>{y.year}</a>
					</h2>
					<p class="quiet">{i18n.t.eventsCount(y.events)}</p>
					{#if y.summary}<p class="period-summary">{y.summary}</p>{/if}
					{@render monthStrip(y)}
					{@render people(y.people)}
				</li>
			{/each}
		</ol>
	{/if}
</div>
