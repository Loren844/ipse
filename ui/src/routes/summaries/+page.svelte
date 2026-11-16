<script lang="ts">
	import { page } from '$app/state';
	import { goto } from '$app/navigation';
	import { tick } from 'svelte';
	import { api, type SummariesView, type SummaryView } from '#lib/api.ts';
	import { dayOf } from '#lib/format.ts';
	import { i18n } from '#lib/i18n.svelte.ts';
	import SummaryCard from '#lib/SummaryCard.svelte';

	type Period = 'day' | 'week' | 'month' | 'year';
	const period: Period = $derived.by(() => {
		const asked = page.url.searchParams.get('period');
		return asked === 'week' || asked === 'month' || asked === 'year' ? asked : 'day';
	});
	const before = $derived(page.url.searchParams.get('before') ?? undefined);
	const periods = $derived([
		{ value: 'day' as Period, label: i18n.t.viewDays },
		{ value: 'week' as Period, label: i18n.t.viewWeeks },
		{ value: 'month' as Period, label: i18n.t.viewMonths },
		{ value: 'year' as Period, label: i18n.t.viewYears }
	]);
	const today = dayOf(new Date().toISOString());

	let items: SummaryView[] = $state([]);
	let view: SummariesView | null = $state(null);
	let failed = $state(false);

	async function load(offset = 0) {
		try {
			const next = await api.summaries(period, offset, before);
			items = offset ? [...items, ...next.items] : next.items;
			view = next;
			failed = false;
			if (!offset && page.url.hash) {
				await tick();
				document.getElementById(page.url.hash.slice(1))?.scrollIntoView();
			}
		} catch {
			failed = true;
		}
	}

	$effect(() => {
		// Reload when the period or the starting day changes.
		void period;
		void before;
		view = null;
		load();
	});
</script>

<svelte:head><title>{i18n.t.summariesNav} · Ipse</title></svelte:head>

<div class="screen">
	<header class="page-head">
		<h1>{i18n.t.summariesNav}</h1>
		<p class="quiet">{i18n.t.summariesLead}</p>
		<div class="views periods" role="navigation" aria-label={i18n.t.summariesNav}>
			{#each periods as choice (choice.value)}
				<a
					href="/summaries{choice.value === 'day' ? '' : `?period=${choice.value}`}"
					aria-current={choice.value === period ? 'page' : undefined}
					onclick={(e) => {
						e.preventDefault();
						goto(e.currentTarget.getAttribute('href') ?? '/summaries', { replace: true, reset: false });
					}}>{choice.label}</a
				>
			{/each}
		</div>
	</header>

	{#if view && view.waiting > 0}
		<a class="banner" href="/review">
			<span>{i18n.t.summariesWaiting(view.waiting)}</span>
			<span class="action">{i18n.t.review}</span>
		</a>
	{/if}

	{#if failed}
		<p class="quiet">{i18n.t.failed}</p>
	{:else if view && !items.length}
		<div class="empty-thread">
			{#if period === 'day'}
				<p class="lead">{i18n.t.noSummaries}</p>
				<p>{i18n.t.noSummariesHint} <a href="/ais">{i18n.t.noSummariesHintLink}</a>.</p>
			{:else}
				<p>{i18n.t.noSummariesOfPeriod}</p>
			{/if}
		</div>
	{:else if view}
		<div class="summaries" data-testid="summaries">
			{#each items as summary, i (summary.start)}
				<SummaryCard bind:summary={items[i]} {today} />
			{/each}
		</div>
		{#if view.more}
			<button type="button" class="more" onclick={() => load(items.length)}>{i18n.t.olderSummaries}</button>
		{/if}
	{/if}
</div>
