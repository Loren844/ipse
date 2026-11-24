<script lang="ts">
	import FirstVisit from '#lib/FirstVisit.svelte';
	import { onMount, untrack } from 'svelte';
	import { page } from '$app/state';
	import { goto, replaceState } from '$app/navigation';
	import { api, type GraphView, type MemoryView, type SearchView } from '#lib/api.ts';
	import { dayOf, hrefOf, shortDay } from '#lib/format.ts';
	import { i18n } from '#lib/i18n.svelte.ts';
	import { icons } from '#lib/icons.ts';
	import { iconOf, KINDS } from '#lib/kinds.ts';
	import MemoryGraph, { SHAPES } from '#lib/MemoryGraph.svelte';
	import { layoutMemory, REGION_KINDS } from '#lib/memoryGraph.ts';
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

	// The graph of the whole memory (issue #174), beside the list: `?graph=1`, centred on
	// `id`, filtered by `kinds` and `hide` (categories).
	const open = $derived(page.url.searchParams.get('graph') === '1');
	let graph = $state.raw<GraphView | null>(null);
	let graphFailed = $state(false);
	let drawing: ReturnType<typeof MemoryGraph> | undefined = $state();
	$effect(() => {
		if (!open || graph) return;
		api.graph().then(
			(value) => (graph = value),
			() => (graphFailed = true)
		);
	});
	const laid = $derived(graph ? layoutMemory(graph) : null);
	const indexOf = $derived(new Map(graph?.nodes.map((n, i) => [n.id, i]) ?? []));
	const kinds = $derived(page.url.searchParams.get('kinds')?.split(',').filter(Boolean) ?? [...REGION_KINDS]);
	const hidden = $derived(page.url.searchParams.get('hide')?.split(',').filter(Boolean) ?? []);
	const shown = $derived.by(() => {
		const keep = new Set(kinds);
		const hide = new Set(hidden);
		const nodes = graph?.nodes ?? [];
		return (i: number) => {
			const node = nodes[i];
			return !!node && keep.has(node.kind) && !hide.has(node.category);
		};
	});
	// The chosen file: the address says it, and this page changes the address itself.
	let chosenId = $state(page.url.searchParams.get('id'));
	$effect(() => {
		chosenId = page.url.searchParams.get('id');
	});
	const chosen = $derived.by(() => {
		const id = chosenId;
		const at = id ? indexOf.get(id) : undefined;
		return at ?? (graph?.centered ? 0 : -1);
	});
	const file = $derived(graph && chosen >= 0 ? graph.nodes[chosen] : null);
	const isMe = $derived(chosen === 0 && !!graph?.centered);

	/** The chosen file's links, among what the filters show: by kind, then as listed. */
	const linked = $derived.by(() => {
		if (!graph || !laid || chosen < 0) return { shown: [] as number[], hidden: 0 };
		const all = Array.from(laid.links.subarray(laid.start[chosen]!, laid.start[chosen + 1]!));
		const order = (i: number) => (REGION_KINDS as readonly string[]).indexOf(graph!.nodes[i]!.kind);
		const kept = all.filter(shown).sort((a, b) => order(a) - order(b) || a - b);
		return { shown: kept, hidden: all.length - kept.length };
	});
	const LISTED = 60;

	/** The address of this screen with some of its choices changed. */
	function address(change: Record<string, string | null>): URL {
		const url = new URL(page.url.href);
		for (const [key, value] of Object.entries(change)) {
			if (value === null) url.searchParams.delete(key);
			else url.searchParams.set(key, value);
		}
		return url;
	}

	function toggleGraph() {
		goto(address({ graph: open ? null : '1', id: open ? null : page.url.searchParams.get('id') }), { reset: false });
	}

	/** Chooses the file `i`; `center` moves the view to it too. */
	function choose(i: number, center = false) {
		if (!graph) return;
		const id = graph.nodes[i]?.id;
		if (!id) return;
		const mine = i === 0 && graph.centered;
		chosenId = mine ? null : id;
		replaceState(address({ id: mine ? null : id }), {});
		if (center) drawing?.centerOn(i);
	}

	function toggleKind(kind: string) {
		const next = kinds.includes(kind) ? kinds.filter((k) => k !== kind) : REGION_KINDS.filter((k) => k === kind || kinds.includes(k));
		goto(address({ kinds: next.length === REGION_KINDS.length ? null : next.join(',') }), { reset: false, replace: true });
	}

	function toggleCategory(name: string) {
		const next = hidden.includes(name) ? hidden.filter((c) => c !== name) : [...hidden, name];
		goto(address({ hide: next.length ? next.join(',') : null }), { reset: false, replace: true });
	}

	/** In the list, with the graph open: a file is chosen in the graph rather than opened. */
	function picked(event: MouseEvent, id: string) {
		const at = indexOf.get(id);
		if (!open || at === undefined || event.metaKey || event.ctrlKey || event.shiftKey) return;
		event.preventDefault();
		choose(at, true);
	}

	const nameOf = (i: number) => (i === 0 && graph?.centered ? i18n.t.graphYou : (graph?.nodes[i]?.name ?? ''));
</script>

{#snippet mark(kind: string, hollow: boolean, you = false)}
	<svg class="graph-shape" class:hollow class:you viewBox="-6 -6 12 12" width="12" height="12" aria-hidden="true">
		{#if you}<circle r="5" />
		{:else if SHAPES[kind] === 'square'}<rect x="-4.2" y="-4.2" width="8.4" height="8.4" />
		{:else if SHAPES[kind] === 'diamond'}<path d="M0-5.5 5.5 0 0 5.5-5.5 0Z" />
		{:else if SHAPES[kind] === 'triangle'}<path d="M0-5.5 5 3.8H-5Z" />
		{:else if SHAPES[kind] === 'down'}<path d="M0 5.5 5-3.8H-5Z" />
		{:else if SHAPES[kind] === 'hexagon'}<path d="M5 0 2.5 4.3h-5L-5 0l2.5-4.3h5Z" />
		{:else if SHAPES[kind] === 'cross'}<path d="M-5-2h10v4H-5ZM-2-5h4v10h-4Z" />
		{:else if SHAPES[kind] === 'dot'}<circle r="3.2" />
		{:else}<circle r="4.6" />{/if}
	</svg>
{/snippet}

<svelte:head><title>{i18n.t.memory} · Ipse</title></svelte:head>

<div class="screen" class:with-graph={open}>
	<header class="page-head">
		<div class="head-row">
			<h1>{i18n.t.memory}</h1>
			<button type="button" class="toggle" aria-pressed={open} onclick={toggleGraph}
				><svg viewBox="0 0 24 24" width="15" height="15" aria-hidden="true"><path d={icons.graph} /></svg>{i18n.t.graphView}</button
			>
			<a class="button small" href="/memory/new">+ {i18n.t.newFile}</a>
		</div>
		<p class="intro">{i18n.t.memoryLead}</p>
	</header>
	<FirstVisit screen="memory" />

	<div class={open ? 'memory-split' : 'memory-whole'}>
	<div class="memory-list">

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
							<a href={href ?? '/summaries'} onclick={(event) => picked(event, item.id)} aria-current={file?.id === item.id ? 'true' : undefined}>
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

	{#if open}
		<section class="memory-graph" aria-labelledby="graph-title">
			<h2 id="graph-title" class="visually-hidden">{i18n.t.graph}</h2>
			{#if graph}
				<details class="graph-filters-box" open={kinds.length < REGION_KINDS.length || hidden.length > 0}>
					<summary>{i18n.t.graphFilters}{#if kinds.length < REGION_KINDS.length || hidden.length}<span class="quiet"> · {i18n.t.graphFiltered}</span>{/if}</summary>
				<div class="graph-filters">
					<fieldset>
						<legend>{i18n.t.graphKinds}</legend>
						<div class="toggles">
							{#each REGION_KINDS as kind (kind)}
								<button
									type="button"
									class="toggle"
									aria-pressed={kinds.includes(kind)}
									disabled={kinds.length === 1 && kinds.includes(kind)}
									onclick={() => toggleKind(kind)}>{@render mark(kind, false)}{i18n.t.kinds[kind]}</button
								>
							{/each}
						</div>
					</fieldset>
					<fieldset>
						<legend>{i18n.t.graphCategories}</legend>
						<div class="toggles">
							{#each graph.categories as category (category.name)}
								<button
									type="button"
									class="toggle"
									aria-pressed={!hidden.includes(category.name)}
									disabled={!hidden.includes(category.name) && hidden.length >= graph.categories.length - 1}
									onclick={() => toggleCategory(category.name)}
									>{category.label ?? i18n.category(category.name)}{#if category.sensitive}<svg
											class="toggle-lock"
											viewBox="0 0 24 24"
											width="13"
											height="13"
											role="img"
											aria-label={i18n.t.hiddenFromAis}><rect x="5" y="10.5" width="14" height="10" rx="2.5" /><path d="M8.5 10.5V7.5a3.5 3.5 0 0 1 7 0v3" /></svg
										>{/if}</button
								>
							{/each}
						</div>
					</fieldset>
				</div>
				</details>
			{/if}
			<div class="graph-stage">
				{#if graph && laid}
					<MemoryGraph bind:this={drawing} {graph} {laid} {shown} {chosen} onchoose={(i) => choose(i)} />
				{:else if graphFailed}
					<p class="graph-loading">{i18n.t.failed}</p>
				{:else}
					<p class="graph-loading" role="status">{i18n.t.graphLoading}</p>
				{/if}
			</div>
			{#if graph}
				<div class="graph-panel" data-testid="graph-panel" aria-live="polite">
					{#if file}
						<h2>{file.name}</h2>
						<p class="facts">
							<span>{@render mark(file.kind, file.sensitive, isMe)} {isMe ? i18n.t.graphYou : (i18n.t.kind[file.kind] ?? file.kind)}</span>
							<span>{i18n.category(file.category)}</span>
							{#if file.sensitive}<span>{i18n.t.hiddenFromAis}</span>{/if}
							{#if file.start}<span>{shortDay(dayOf(file.start))}</span>{/if}
						</p>
						<div class="actions">
							{#if hrefOf({ id: file.id, kind: file.kind })}
								<a class="button small" href={hrefOf({ id: file.id, kind: file.kind })}>{i18n.t.openPage}</a>
							{/if}
							<button type="button" class="button small" onclick={() => choose(chosen, true)}>{i18n.t.centerOn(nameOf(chosen))}</button>
						</div>
						<h3>{i18n.t.graphLinks(linked.shown.length)}</h3>
						{#if linked.shown.length}
							<ul data-testid="graph-links">
								{#each linked.shown.slice(0, LISTED) as j (j)}
									{@const other = graph.nodes[j]!}
									<li>
										<button type="button" aria-label={i18n.t.chooseInGraph(nameOf(j))} onclick={() => choose(j, true)}>
											{@render mark(other.kind, other.sensitive, j === 0 && graph.centered)}
											<span class="name">{nameOf(j)}</span>
											<span class="quiet">{other.start ? shortDay(dayOf(other.start)) : (i18n.t.kind[other.kind] ?? other.kind)}</span>
										</button>
									</li>
								{/each}
							</ul>
							{#if linked.shown.length > LISTED}<p class="quiet">{i18n.t.graphMoreLinks(linked.shown.length - LISTED)}</p>{/if}
						{/if}
						{#if linked.hidden}<p class="quiet">{i18n.t.graphLinksHidden}</p>{/if}
					{:else}
						<p class="quiet">{i18n.t.graphChoose}</p>
					{/if}
					<div class="graph-legend">
						<h3 class="visually-hidden">{i18n.t.graphLegend}</h3>
						<ul aria-label={i18n.t.graphLegend}>
							{#if graph.centered}<li>{@render mark('person', false, true)}{i18n.t.graphYou}</li>{/if}
							{#each REGION_KINDS as kind (kind)}<li>{@render mark(kind, false)}{i18n.t.kinds[kind]}</li>{/each}
							<li>{@render mark('person', true)}{i18n.t.hiddenFromAis}</li>
						</ul>
					</div>
				</div>
			{/if}
		</section>
	{/if}
	</div>
</div>
