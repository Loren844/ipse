<script lang="ts">
	// The view around someone (issue #244): one person or place, `?id=`, and everything
	// the memory holds about them, by kind and by time. Each section shows its first
	// items and how many there are; "see all" pages through the rest. The graph is one
	// click away.
	import { page } from '$app/state';
	import { api, NotFound, SessionEnded, type AroundEventView, type AroundView, type LinkedView, type SharedView } from '#lib/api.ts';
	import { app } from '#lib/app.svelte.ts';
	import Face from '#lib/Face.svelte';
	import { dayOf, hrefOf, isDay, longDay, shortDay, when } from '#lib/format.ts';
	import { i18n } from '#lib/i18n.svelte.ts';
	import { iconOf } from '#lib/kinds.ts';
	import Mention from '#lib/Mention.svelte';
	import { Loader } from '#lib/remote.svelte.ts';
	import Sensitive from '#lib/Sensitive.svelte';

	type Section = 'events' | 'visits' | 'places' | 'people' | 'remembered' | 'reminders' | 'linked';

	const remote = new Loader<AroundView>();
	const id = $derived(page.url.searchParams.get('id') ?? '');
	/** The items "see all" brought, by section: they replace the first ones. */
	let paged: Partial<Record<Section, unknown[]>> = $state({});
	let pending: Section | null = $state(null);
	$effect(() => {
		const asked = id;
		paged = {};
		remote.load(() => api.around(asked));
	});
	const view = $derived(remote.state.kind === 'ready' ? remote.state.value : null);
	const person = $derived(view?.center.kind === 'person');
	const place = $derived(view?.center.kind === 'place');
	const today = dayOf(new Date().toISOString());
	const locale = $derived(i18n.language === 'fr' ? 'fr-FR' : 'en-GB');

	function items<T>(section: Section, first: T[]): T[] {
		return (paged[section] as T[] | undefined) ?? first;
	}

	/** Brings the section's next items: from the start the first time. */
	async function more(section: Section) {
		if (!view || pending) return;
		const shown = paged[section]?.length ?? 0;
		pending = section;
		try {
			const next = await api.around(id, section, shown);
			paged = { ...paged, [section]: [...(paged[section] ?? []), ...next[section].items] };
		} catch (error) {
			if (error instanceof SessionEnded) app.state = { kind: 'ended' };
			if (error instanceof NotFound) remote.state = { kind: 'missing' };
		} finally {
			pending = null;
		}
	}

	/** The strip: by month over two years at most, else by year; empty periods too. */
	const strip = $derived.by(() => {
		const months = view?.months ?? [];
		if (!months.length) return null;
		const [fy, fm] = months[0].month.split('-').map(Number);
		const [ly, lm] = months[months.length - 1].month.split('-').map(Number);
		const span = (ly - fy) * 12 + (lm - fm) + 1;
		const count = new Map(months.map((m) => [m.month, m.events]));
		if (span <= 24) {
			const cells = Array.from({ length: span }, (_, i) => {
				const y = fy + Math.floor((fm - 1 + i) / 12);
				const m = ((fm - 1 + i) % 12) + 1;
				const key = `${y}-${String(m).padStart(2, '0')}`;
				return { key, year: y, month: key.slice(5), events: count.get(key) ?? 0, label: m === 1 || (i === 0 && fm <= 9) ? String(y) : '' };
			});
			return { byMonth: true, cells };
		}
		const years = new Map<number, number>();
		for (const m of months) years.set(Number(m.month.slice(0, 4)), (years.get(Number(m.month.slice(0, 4))) ?? 0) + m.events);
		const cells = Array.from({ length: ly - fy + 1 }, (_, i) => {
			const y = fy + i;
			return { key: String(y), year: y, month: null as string | null, events: years.get(y) ?? 0, label: String(y) };
		});
		return { byMonth: false, cells };
	});
	const fullest = $derived(Math.max(1, ...(strip?.cells.map((c) => c.events) ?? [1])));

	const monthTitle = (key: string) =>
		new Intl.DateTimeFormat(locale, { timeZone: 'UTC', month: 'long', year: 'numeric' }).format(new Date(`${key}-01T12:00:00Z`));

	/** The agenda's Years view at that period, on this file's events only (issue #211). */
	function yearsAt(year: number, month: string | null): string {
		const query = new URLSearchParams({ view: 'years', year: String(year) });
		if (month) query.set('month', month);
		query.set('person', id);
		return `/agenda?${query}`;
	}

	const day = (text: string) => shortDay(dayOf(text));
	const kindName = (kind: string) => i18n.t.kind[kind] ?? kind;
	const link = (file: { id: string; name: string; kind: string }) => ({ id: file.id, name: file.name, kind: file.kind });
</script>

{#snippet seeAll(section: Section, total: number, shown: number)}
	{#if total > shown}
		<button type="button" class="link-button" disabled={pending === section} onclick={() => more(section)}
			>{paged[section] ? i18n.t.aroundShowMore : i18n.t.aroundSeeAll(total)}</button
		>
	{/if}
{/snippet}

{#snippet hidden(sensitive: boolean)}
	{#if sensitive}<span class="lock" title={i18n.t.hiddenFromAis}
			><svg viewBox="0 0 24 24" width="13" height="13" aria-hidden="true"
				><rect x="5" y="10.5" width="14" height="10" rx="2.5" /><path d="M8.5 10.5V7.5a3.5 3.5 0 0 1 7 0v3" /></svg
			><span class="visually-hidden">{i18n.t.hiddenFromAis}</span></span
		>{/if}
{/snippet}

{#snippet eventList(section: Section, list: AroundEventView[], total: number)}
	<ul class="around-list" data-testid="around-{section}">
		{#each list as event (event.file.id)}
			<li>
				<a href="/agenda/{event.file.id}">{event.file.name}</a>{@render hidden(event.file.sensitive)}
				<span class="quiet">{event.file.start ? (isDay(event.file.start) ? longDay(event.file.start, today) : when(event.file.start, today)) : ''}</span>
				{#if event.kind}<span class="kind-tag">{i18n.t.eventKinds[event.kind] ?? event.kind}</span>{/if}
				{#if event.place}<Mention link={link(event.place)} />{/if}
			</li>
		{/each}
	</ul>
	{@render seeAll(section, total, list.length)}
{/snippet}

{#snippet sharedList(section: Section, list: SharedView[], total: number)}
	<ul class="around-list" data-testid="around-{section}">
		{#each list as shared (shared.file.id)}
			<li>
				<Mention link={link(shared.file)} />{@render hidden(shared.file.sensitive)}
				<span class="quiet">{i18n.t.aroundTimes(shared.events)} · {i18n.t.graphLast(day(shared.last))}</span>
			</li>
		{/each}
	</ul>
	{@render seeAll(section, total, list.length)}
{/snippet}

{#snippet linkedList(section: Section, list: LinkedView[], total: number)}
	{#if list.length}
		<ul class="around-list" data-testid="around-{section}">
			{#each list as linked (linked.file.id)}
				<li>
					<Mention link={link(linked.file)} />{@render hidden(linked.file.sensitive)}
					<span class="quiet">{kindName(linked.file.kind)}{#if linked.word} · {linked.word}{/if}</span>
				</li>
			{/each}
		</ul>
		{@render seeAll(section, total, list.length)}
	{/if}
{/snippet}

<svelte:head>{#if view}<title>{i18n.t.aroundTitle(view.center.name)} · Ipse</title>{/if}</svelte:head>

{#if view}
	{@const events = items('events', view.events.items)}
	{@const visits = items('visits', view.visits.items)}
	{@const places = items('places', view.places.items)}
	{@const people = items('people', view.people.items)}
	{@const remembered = items('remembered', view.remembered.items)}
	{@const reminders = items('reminders', view.reminders.items)}
	{@const linked = items('linked', view.linked.items)}
	{@const empty = !view.total && !view.facts.length && !view.notes && !view.linked.total && !view.remembered.total && !view.reminders.total && !view.photos && !view.stays}
	<article class="screen around-screen">
		<header class="person-head">
			{#if person}
				<Face name={view.center.name} size={72} />
			{:else}
				<span class="kind-icon big" aria-hidden="true"
					><svg viewBox="0 0 24 24" width="34" height="34"><path d={iconOf(view.center.kind)} /></svg></span
				>
			{/if}
			<div>
				<h1>{view.center.name}</h1>
				<p class="date" data-testid="around-what">
					{[view.mine ? i18n.t.graphYourFile : kindName(view.center.kind), ...view.relation].join(' · ')}
				</p>
			</div>
			<div class="head-actions">
				{#if hrefOf(link(view.center))}<a class="button small" href={hrefOf(link(view.center))}>{i18n.t.aroundOpenFile}</a>{/if}
				<a class="button small" href="/graph?id={view.center.id}" aria-label={i18n.t.aroundInGraphLabel(view.center.name)}>{i18n.t.aroundInGraph}</a>
			</div>
		</header>

		{#if view.center.sensitive}<Sensitive category={view.center.category} />{/if}

		{#if view.total}
			<ul class="around-facts" data-testid="around-summary">
				<li>{i18n.t.aroundCount(view.total, view.this_year)}</li>
				{#if view.first && view.last}<li>{i18n.t.aroundSeen(day(view.first), day(view.last), person)}</li>{/if}
				{#if view.next}<li>{i18n.t.aroundNext(day(view.next))}</li>{/if}
			</ul>
		{/if}

		{#if empty}<p class="quiet">{i18n.t.aroundNothing}</p>{/if}

		{#if strip}
			<section aria-labelledby="around-strip">
				<h2 id="around-strip">{i18n.t.aroundStrip(person)}</h2>
				<ol class="strip around-strip" style="grid-template-columns: repeat({strip.cells.length}, minmax(0, 1fr))" data-testid="around-strip">
					{#each strip.cells as cell (cell.key)}
						<li>
							{#if cell.events}
								<a
									href={yearsAt(cell.year, cell.month)}
									aria-label="{strip.byMonth ? monthTitle(cell.key) : cell.key}, {i18n.t.eventsCount(cell.events)}"
								>
									<span class="bar" style="--fill: {Math.max(0.08, cell.events / fullest)}"></span>
									<span class="strip-label" aria-hidden="true">{cell.label}</span>
								</a>
							{:else}
								<span class="bar empty" aria-hidden="true"></span>
								<span class="strip-label" aria-hidden="true">{cell.label}</span>
							{/if}
						</li>
					{/each}
				</ol>
				<p class="quiet hint">{i18n.t.aroundStripHint}</p>
			</section>
		{/if}

		{#if view.events.total}
			<section aria-labelledby="around-events">
				<h2 id="around-events">{i18n.t.aroundEvents} <span class="count-tag">{view.events.total}</span></h2>
				{@render eventList('events', events, view.events.total)}
			</section>
		{/if}

		{#if place && (view.visits.total || view.stays)}
			<section aria-labelledby="around-visits">
				<h2 id="around-visits">{i18n.t.aroundVisits}{#if view.visits.total} <span class="count-tag">{view.visits.total}</span>{/if}</h2>
				{#if view.stays}
					<p data-testid="around-stays">{i18n.t.aroundStays(view.stays.count, day(view.stays.first), day(view.stays.last))}{@render hidden(view.stays.sensitive)}</p>
				{/if}
				{#if view.visits.total}{@render eventList('visits', visits, view.visits.total)}{/if}
			</section>
		{/if}

		{#if view.places.total}
			<section aria-labelledby="around-places">
				<h2 id="around-places">{i18n.t.aroundPlaces}</h2>
				{@render sharedList('places', places, view.places.total)}
			</section>
		{/if}

		{#if view.photos}
			<section aria-labelledby="around-photos">
				<h2 id="around-photos">{i18n.t.aroundPhotos}</h2>
				<p data-testid="around-photos">{i18n.t.aroundPhotosCount(view.photos.count, day(view.photos.first), day(view.photos.last))}{@render hidden(view.photos.sensitive)}</p>
				<p class="quiet hint">{i18n.t.aroundPhotosHint}</p>
			</section>
		{/if}

		{#if view.facts.length || view.remembered.total}
			<section aria-labelledby="around-remembered">
				<h2 id="around-remembered">{i18n.t.aroundRemembered}</h2>
				{#if view.facts.length}
					<ul class="facts" data-testid="around-facts">{#each view.facts as fact, i (i)}<li>{fact}</li>{/each}</ul>
				{/if}
				{@render linkedList('remembered', remembered, view.remembered.total)}
			</section>
		{/if}

		{#if view.reminders.total}
			<section aria-labelledby="around-reminders">
				<h2 id="around-reminders">{i18n.t.aroundReminders}</h2>
				<ul class="around-list" data-testid="around-reminders">
					{#each reminders as reminder (reminder.file.id)}
						<li>
							<Mention link={link(reminder.file)} />{@render hidden(reminder.file.sensitive)}
							{#if reminder.done}<span class="quiet">{i18n.t.aroundDone(shortDay(reminder.done))}</span>
							{:else if reminder.at}<span class="quiet">{i18n.t.aroundDue(day(reminder.at))}</span>{/if}
						</li>
					{/each}
				</ul>
				{@render seeAll('reminders', view.reminders.total, reminders.length)}
			</section>
		{/if}

		{#if view.notes || view.linked.total}
			<section aria-labelledby="around-notes">
				<h2 id="around-notes">{i18n.t.aroundNotes}</h2>
				{#if view.notes}<p class="notes">{view.notes}</p>{/if}
				{@render linkedList('linked', linked, view.linked.total)}
			</section>
		{/if}

		{#if view.people.total}
			<section aria-labelledby="around-people">
				<h2 id="around-people">{i18n.t.aroundPeople(person)}</h2>
				{@render sharedList('people', people, view.people.total)}
			</section>
		{/if}
	</article>
{:else if remote.state.kind === 'missing' || (remote.state.kind === 'ready' && !view)}
	<div class="screen"><h1>{i18n.t.notFound}</h1><p class="quiet">{i18n.t.aroundMissing}</p><p><a href="/memory">{i18n.t.memory}</a></p></div>
{:else if remote.state.kind === 'failed'}
	<p class="quiet">{i18n.t.failed}</p>
{/if}
