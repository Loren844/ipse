<script lang="ts">
	import { page } from '$app/state';
	import { goto } from '$app/navigation';
	import HistoryAccesses from '#lib/HistoryAccesses.svelte';
	import HistoryChanges from '#lib/HistoryChanges.svelte';
	import HistoryRuns from '#lib/HistoryRuns.svelte';
	import { i18n } from '#lib/i18n.svelte.ts';

	type View = 'changes' | 'accesses' | 'runs';
	const view: View = $derived.by(() => {
		const asked = page.url.searchParams.get('view');
		return asked === 'accesses' || asked === 'runs' ? asked : 'changes';
	});
	const views = $derived([
		{ value: 'changes' as View, label: i18n.t.viewChanges },
		{ value: 'accesses' as View, label: i18n.t.viewAccesses },
		{ value: 'runs' as View, label: i18n.t.viewRuns }
	]);
	const current = $derived(views.find((v) => v.value === view)!);
</script>

<svelte:head><title>{i18n.t.history} · Ipse</title></svelte:head>

<div class="screen">
	<header class="page-head">
		<h1>{i18n.t.history}</h1>
		<p class="quiet">{i18n.t.historyLead}</p>
		<div class="views" role="navigation" aria-label={i18n.t.history}>
			{#each views as choice (choice.value)}
				<a
					href="/history{choice.value === 'changes' ? '' : `?view=${choice.value}`}"
					aria-current={choice.value === view ? 'page' : undefined}
					onclick={(e) => {
						e.preventDefault();
						goto(e.currentTarget.getAttribute('href') ?? '/history', { replace: true, reset: false });
					}}>{choice.label}</a
				>
			{/each}
		</div>
	</header>

	<section aria-label={current.label}>
		{#key view}
			{#if view === 'accesses'}
				<HistoryAccesses />
			{:else if view === 'runs'}
				<HistoryRuns />
			{:else}
				<HistoryChanges />
			{/if}
		{/key}
	</section>
</div>
