<script lang="ts" module>
	/** The files visited, to go back: kept while the app is open. */
	let trail: { id: string; name: string }[] = $state([]);
	const TRAIL = 8;
</script>

<script lang="ts">
	// The graph centred on one file (issue #213): `?id=` (the owner without one), its
	// direct links, the strongest first. Events are not drawn: two files in the same
	// events are linked by how many there are, in the last twelve months (`period=all`:
	// since always). A click on a neighbour makes it the center.
	import EgoGraph from '#lib/EgoGraph.svelte';
	import FirstVisit from '#lib/FirstVisit.svelte';
	import { untrack } from 'svelte';
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { api, type GraphView, type NeighbourView } from '#lib/api.ts';
	import { NEIGHBOUR_KINDS, SHAPES, STEP } from '#lib/egoGraph.ts';
	import { dayOf, hrefOf, shortDay } from '#lib/format.ts';
	import { i18n } from '#lib/i18n.svelte.ts';
	import { Loader } from '#lib/remote.svelte.ts';

	const remote = new Loader<GraphView>();
	const id = $derived(page.url.searchParams.get('id') ?? '');
	const period = $derived(page.url.searchParams.get('period') === 'all' ? 'all' : 'year');
	const kinds = $derived(page.url.searchParams.get('kinds')?.split(',').filter(Boolean) ?? [...NEIGHBOUR_KINDS]);

	// The last answer stays drawn while the next one comes, so that the drawing glides.
	let graph = $state.raw<GraphView | null>(null);
	$effect(() => {
		const asked = id;
		const counted = period;
		remote.load(() => api.graph(asked || undefined, counted === 'all' ? 'all' : 'year'), true);
	});
	$effect(() => {
		if (remote.state.kind !== 'ready') return;
		const value = remote.state.value;
		graph = value;
		const name = value.mine ? i18n.t.graphYou : value.center.name;
		untrack(() => {
			const at = trail.findIndex((t) => t.id === value.center.id);
			if (at >= 0) trail = trail.slice(0, at + 1);
			else trail = [...trail, { id: value.center.id, name }].slice(-TRAIL);
		});
	});

	let more = $state(0);
	$effect(() => {
		void graph?.center.id;
		more = 0;
	});
	const kept = $derived((graph?.neighbours ?? []).filter((n) => kinds.includes(n.file.kind)));
	const shown = $derived(kept.slice(0, STEP * (more + 1)));
	const filtered = $derived((graph?.neighbours.length ?? 0) - kept.length);
	let highlighted: string | null = $state(null);
	const lit = $derived(graph?.neighbours.find((n) => n.file.id === highlighted) ?? null);
	const centerName = $derived(graph ? (graph.mine ? i18n.t.graphYou : graph.center.name) : '');

	/** The address of this screen with some of its choices changed. */
	function address(change: Record<string, string | null>): string {
		const url = new URL(page.url.href);
		for (const [key, value] of Object.entries(change)) {
			if (value === null) url.searchParams.delete(key);
			else url.searchParams.set(key, value);
		}
		return `${url.pathname}${url.search}`;
	}

	function center(next: string) {
		highlighted = null;
		goto(address({ id: next }), { reset: false });
	}

	function toggleKind(kind: string) {
		const next = kinds.includes(kind) ? kinds.filter((k) => k !== kind) : NEIGHBOUR_KINDS.filter((k) => k === kind || kinds.includes(k));
		goto(address({ kinds: next.length === NEIGHBOUR_KINDS.length ? null : next.join(',') }), { replace: true, reset: false });
	}

	function count(value: 'year' | 'all') {
		goto(address({ period: value === 'all' ? 'all' : null }), { replace: true, reset: false });
	}

	/** A neighbour's name: "you" for the owner. */
	const nameOf = (n: NeighbourView) => (n.file.id === graph?.owner ? i18n.t.graphYou : n.file.name);

	/** How a neighbour is linked to the center, in words. */
	function how(n: NeighbourView): string {
		const parts: string[] = [];
		if (n.words.length) parts.push(n.words.join(', '));
		for (const word of n.back) parts.push(i18n.t.graphBack(word, graph?.mine ?? false, centerName));
		if (n.together) {
			const often = i18n.t.graphTogether(n.together, (graph?.mine ?? false) || n.file.id === graph?.owner, period === 'year');
			parts.push(n.last ? `${often}, ${i18n.t.graphLast(shortDay(dayOf(n.last)))}` : often);
		}
		if (!parts.length) parts.push(i18n.t.graphLinked);
		return parts.join(' · ');
	}
</script>

{#snippet mark(kind: string, hollow: boolean)}
	<svg class="graph-shape" class:hollow viewBox="-6 -6 12 12" width="12" height="12" aria-hidden="true">
		{#if SHAPES[kind] === 'square'}<rect x="-4.2" y="-4.2" width="8.4" height="8.4" />
		{:else if SHAPES[kind] === 'diamond'}<path d="M0-5.5 5.5 0 0 5.5-5.5 0Z" />
		{:else if SHAPES[kind] === 'triangle'}<path d="M0-5.5 5 3.8H-5Z" />
		{:else if SHAPES[kind] === 'down'}<path d="M0 5.5 5-3.8H-5Z" />
		{:else if SHAPES[kind] === 'hexagon'}<path d="M5 0 2.5 4.3h-5L-5 0l2.5-4.3h5Z" />
		{:else if SHAPES[kind] === 'cross'}<path d="M-5-2h10v4H-5ZM-2-5h4v10h-4Z" />
		{:else if SHAPES[kind] === 'dot'}<circle r="3.2" />
		{:else}<circle r="4.6" />{/if}
	</svg>
{/snippet}

<svelte:head><title>{i18n.t.graph} · Ipse</title></svelte:head>

<div class="screen wide graph-screen">
	<header class="page-head">
		<h1>{i18n.t.graph}</h1>
		<p class="intro">{i18n.t.graphLead}</p>
	</header>
	<FirstVisit screen="graph" />

	{#if remote.state.kind === 'failed'}
		<p class="quiet">{i18n.t.failed}</p>
	{:else if remote.state.kind === 'missing' && !graph}
		<p class="quiet">{id ? i18n.t.graphMissing : i18n.t.graphEmpty}</p>
	{:else if graph}
		{#if trail.length > 1}
			<nav class="graph-trail" aria-label={i18n.t.graphTrail}>
				<ol>
					{#each trail as step, i (step.id)}
						<li>
							{#if i === trail.length - 1}
								<span aria-current="page">{step.name}</span>
							{:else}
								<a href={address({ id: step.id })}>{step.name}</a>
							{/if}
						</li>
					{/each}
				</ol>
			</nav>
		{/if}

		<div class="graph-filters">
			<fieldset>
				<legend>{i18n.t.graphKinds}</legend>
				<div class="toggles">
					{#each NEIGHBOUR_KINDS as kind (kind)}
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
				<legend>{i18n.t.graphPeriod}</legend>
				<div class="segmented" role="group" aria-label={i18n.t.graphPeriod}>
					<button type="button" aria-pressed={period === 'year'} onclick={() => count('year')}>{i18n.t.graphYear}</button>
					<button type="button" aria-pressed={period === 'all'} onclick={() => count('all')}>{i18n.t.graphAlways}</button>
				</div>
			</fieldset>
		</div>

		<div class="graph-layout">
			<div class="graph-stage">
				<EgoGraph {graph} {shown} {centerName} {nameOf} bind:highlighted onchoose={center} />
				<p class="graph-caption" aria-hidden="true">
					{#if lit}<strong>{nameOf(lit)}</strong> · {how(lit)}{:else}{i18n.t.graphHint}{/if}
				</p>
			</div>

			<section class="graph-panel" data-testid="graph-panel" aria-labelledby="graph-center">
				<h2 id="graph-center">{centerName}</h2>
				<p class="graph-facts">
					<span>{@render mark(graph.mine ? 'person' : graph.center.kind, graph.center.sensitive)} {graph.mine ? i18n.t.graphYourFile : (i18n.t.kind[graph.center.kind] ?? graph.center.kind)}</span>
					{#if graph.center.sensitive}<span>{i18n.t.hiddenFromAis}</span>{/if}
					{#if graph.center.start}<span>{shortDay(dayOf(graph.center.start))}</span>{/if}
				</p>
				{#if hrefOf({ id: graph.center.id, kind: graph.center.kind })}
					<p class="graph-open">
						<a href={hrefOf({ id: graph.center.id, kind: graph.center.kind })}>{i18n.t.openPage}</a>
						{#if graph.center.kind !== 'event'}<a href="/around?id={graph.center.id}">{i18n.t.around}</a>{/if}
					</p>
				{/if}
				<h3>{i18n.t.graphLinks(kept.length)}</h3>
				{#if shown.length}
					<ol class="graph-neighbours" data-testid="graph-links">
						{#each shown as n (n.file.id)}
							<li class:lit={highlighted === n.file.id}>
								<button
									type="button"
									aria-label={i18n.t.centerOn(nameOf(n))}
									aria-describedby="how-{n.file.id}"
									onclick={() => center(n.file.id)}
									onfocus={() => (highlighted = n.file.id)}
									onblur={() => (highlighted = null)}
									onmouseenter={() => (highlighted = n.file.id)}
									onmouseleave={() => (highlighted = null)}
								>
									{@render mark(n.file.kind, n.file.sensitive)}
									<span class="name">{nameOf(n)}</span>
								</button>
								<span class="quiet" id="how-{n.file.id}">{how(n)}</span>
							</li>
						{/each}
					</ol>
					{#if kept.length > shown.length}
						<button type="button" class="button small" onclick={() => more++}>{i18n.t.graphShowMore(Math.min(STEP, kept.length - shown.length))}</button>
					{/if}
				{/if}
				{#if filtered}<p class="quiet">{i18n.t.graphLinksHidden}</p>{/if}
				<div class="graph-legend">
					<h3 class="visually-hidden">{i18n.t.graphLegend}</h3>
					<ul aria-label={i18n.t.graphLegend}>
						{#each NEIGHBOUR_KINDS as kind (kind)}<li>{@render mark(kind, false)}{i18n.t.kinds[kind]}</li>{/each}
						<li>{@render mark('person', true)}{i18n.t.hiddenFromAis}</li>
					</ul>
				</div>
			</section>
		</div>
	{:else}
		<p class="quiet" role="status">{i18n.t.graphLoading}</p>
	{/if}
</div>
