<script lang="ts">
	import FirstVisit from '#lib/FirstVisit.svelte';
	import { onMount, untrack } from 'svelte';
	import { page } from '$app/state';
	import { goto, replaceState } from '$app/navigation';
	import { api, type MemoryView, type SearchView } from '#lib/api.ts';
	import { dayOf, hrefOf } from '#lib/format.ts';
	import { i18n } from '#lib/i18n.svelte.ts';
	import { icons } from '#lib/icons.ts';
	import { iconOf, KINDS } from '#lib/kinds.ts';
	import Now from '#lib/Now.svelte';
	import { Loader } from '#lib/remote.svelte.ts';

	const memory = new Loader<MemoryView>();
	const found = new Loader<SearchView>();
	let query = $state(page.url.searchParams.get('q') ?? '');
	let timer: ReturnType<typeof setTimeout> | undefined;
	// The query this page last wrote in the address, to tell it from one the rail's
	// search box wrote.
	let written = (page.url.searchParams.get('q') ?? '').trim();
	const today = dayOf(new Date().toISOString());

	function search() {
		const q = query.trim();
		if (q) found.load(() => api.search(q), true);
		const url = new URL(page.url.href);
		if (q) url.searchParams.set('q', q);
		else url.searchParams.delete('q');
		written = q;
		replaceState(url, {});
	}

	function typed() {
		clearTimeout(timer);
		timer = setTimeout(search, 120);
	}

	onMount(() => {
		// The graph has a screen of its own again (issue #213): an old address leads there.
		if (page.url.searchParams.get('graph') === '1') {
			const id = page.url.searchParams.get('id');
			goto(`/graph${id ? `?id=${encodeURIComponent(id)}` : ''}`, { replace: true });
			return;
		}
		memory.load(() => api.memory());
		search();
	});

	// The rail's search box changes the address: follow it. Only the address is
	// watched, never what the person types here (which the address follows later).
	$effect(() => {
		const q = page.url.searchParams.get('q') ?? '';
		untrack(() => {
			if (q !== written) {
				query = q;
				search();
			}
		});
	});

	const ready = $derived(memory.state.kind === 'ready' ? memory.state.value : null);
</script>

<svelte:head><title>{i18n.t.memory} · Ipse</title></svelte:head>

<div class="screen">
	<header class="page-head">
		<div class="head-row">
			<h1>{i18n.t.memory}</h1>
			<a class="button small" href="/memory/new">+ {i18n.t.newFile}</a>
		</div>
		<p class="intro">{i18n.t.memoryLead}</p>
	</header>
	<FirstVisit screen="memory" />

	<div class="search">
		<label for="search">{i18n.t.searchMemory}</label>
		<div class="field">
			<svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true"><path d={icons.search} /></svg>
			<input
				id="search"
				type="search"
				autocomplete="off"
				spellcheck="false"
				placeholder={i18n.t.searchMemoryHint}
				bind:value={query}
				oninput={typed}
			/>
		</div>
	</div>

	{#if query.trim()}
		{#if found.state.kind === 'ready'}
			{@const items = found.state.value.items}
			<p class="date" aria-live="polite">{i18n.t.found(items.length)}</p>
			{#if items.length}
				<ul class="people found" data-testid="found">
					{#each items as item (item.id)}
						{@const href = hrefOf({ id: item.id, kind: item.kind })}
						<li>
							<a href={href ?? '/summaries'}>
								<span class="kind-icon" aria-hidden="true"
									><svg viewBox="0 0 24 24" width="18" height="18"><path d={iconOf(item.kind)} /></svg></span
								>
								<span class="who">
									<span class="name">{item.name}</span>
									<span class="aliases">{i18n.t.kind[item.kind] ?? item.kind}{item.excerpt ? ` · ${item.excerpt}` : ''}</span>
								</span>
							</a>
						</li>
					{/each}
				</ul>
			{:else}
				<p class="lead">{i18n.t.nothingFound}</p>
			{/if}
		{/if}
	{:else if ready}
		<div class="memory-overview">
		<ul class="kinds" data-testid="kinds">
			{#each KINDS as kind (kind.kind)}
				{@const count = ready.kinds.find((k) => k.kind === kind.kind)?.count ?? 0}
				<li>
					<a href="/memory/{kind.folder}" class:empty={count === 0}>
						<svg viewBox="0 0 24 24" width="22" height="22" aria-hidden="true"><path d={kind.icon} /></svg>
						<span class="kind-name">{i18n.t.kinds[kind.kind]}</span>
						<span class="kind-count">{count}</span>
					</a>
				</li>
			{/each}
		</ul>

		{#if ready.now.length}
			<section aria-labelledby="now">
				<h2 id="now">{i18n.t.nowTitle}</h2>
				<Now states={ready.now} {today} />
			</section>
		{/if}
		</div>
	{:else if memory.state.kind === 'failed'}
		<p class="quiet">{i18n.t.failed}</p>
	{/if}
</div>
