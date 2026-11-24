<script lang="ts">
	// The timeline (issue #133): a life by years, then months. A year's strip shows how
	// full each month was; a month opens what marked it, then its calendar. It is the
	// agenda's Years view (issue #211): its place is in the address, `/agenda?view=years`
	// with `year` and `month`.
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { api, type MonthView, type TimelineView, type YearView } from './api.ts';
	import { capitalize, dayOf, longDay } from './format.ts';
	import { i18n } from './i18n.svelte.ts';
	import Mention from './Mention.svelte';
	import { Loader } from './remote.svelte.ts';

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

	/** The address of the years, a year or a month of the timeline. */
	function at(next: { year?: number | null; month?: string | null }): string {
		const query = new URLSearchParams({ view: 'years' });
		if (next.year) query.set('year', String(next.year));
		if (next.year && next.month) query.set('month', next.month.slice(5));
		return `/agenda?${query}`;
	}

	function open(next: { year?: number | null; month?: string | null }) {
		goto(at(next), { reset: false });
	}
</script>

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
						href={at({ year: y.year, month: cell.key })}
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

<div class="timeline-screen">
	<header class="timeline-head">
		<nav class="crumbs" aria-label={i18n.t.whereInTimeline}>
			{#if year}
				<a href={at({})} onclick={(e) => (e.preventDefault(), open({}))}>{i18n.t.timeline}</a>
				{#if chosenMonth}
					<span aria-hidden="true">›</span>
					<a href={at({ year })} onclick={(e) => (e.preventDefault(), open({ year }))}>{year}</a>
				{/if}
			{/if}
		</nav>
		{#if year}
			<h2 class="period">{chosenMonth ? monthTitle(chosenMonth.month) : chosenYear ? chosenYear.year : i18n.t.timeline}</h2>
		{:else}
			<p class="quiet">{i18n.t.timelineLead}</p>
		{/if}
	</header>

	{#if remote.state.kind === 'failed'}
		<p class="quiet">{i18n.t.failed}</p>
	{:else if view && !view.years.length}
		<p class="quiet">{i18n.t.timelineEmpty}</p>
	{:else if chosenMonth}
		{#key chosenMonth.month}<section class="period-detail appear" aria-label={monthTitle(chosenMonth.month)}>
			<p class="period-count">{i18n.t.eventsCount(chosenMonth.events)}</p>
			{#if chosenMonth.summary}<p class="period-summary">{chosenMonth.summary}</p>{/if}
			{#if chosenMonth.marks.length}
				<h3>{i18n.t.whatMarked}</h3>
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
		</section>{/key}
	{:else if chosenYear}
		{#key chosenYear.year}<section class="period-detail appear" aria-label={String(chosenYear.year)}>
			<p class="period-count">{i18n.t.eventsCount(chosenYear.events)}</p>
			{#if chosenYear.summary}<p class="period-summary">{chosenYear.summary}</p>{/if}
			{@render people(chosenYear.people)}
			{@render monthStrip(chosenYear)}
			<ol class="months">
				{#each chosenYear.months as m (m.month)}
					<li class="month-card">
						<h3>
							<a href={at({ year: chosenYear.year, month: m.month })} onclick={(e) => (e.preventDefault(), open({ year: chosenYear.year, month: m.month }))}
								>{monthName(m.month)}</a
							>
						</h3>
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
		</section>{/key}
	{:else if view}
		<ol class="years appear">
			{#each view.years as y (y.year)}
				<li class="year-row">
					<h3>
						<a href={at({ year: y.year })} onclick={(e) => (e.preventDefault(), open({ year: y.year }))}>{y.year}</a>
					</h3>
					<p class="quiet">{i18n.t.eventsCount(y.events)}</p>
					{#if y.summary}<p class="period-summary">{y.summary}</p>{/if}
					{@render monthStrip(y)}
					{@render people(y.people)}
				</li>
			{/each}
		</ol>
	{/if}
</div>
