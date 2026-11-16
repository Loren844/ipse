<script lang="ts">
	// The graph (issue #130): what a file links to and what links to it, one or two
	// steps away, drawn around it. The drawing is for the eye and the mouse; the same
	// neighbourhood follows as a list, for the keyboard and screen readers.
	import { page } from '$app/state';
	import { goto } from '$app/navigation';
	import { Tween } from 'svelte/motion';
	import { cubicOut } from 'svelte/easing';
	import { api, type GraphView, type NodeView } from '#lib/api.ts';
	import { dayOf, hrefOf, shortDay } from '#lib/format.ts';
	import { HEIGHT, WIDTH, labelSide, layout, type Spot } from '#lib/graphLayout.ts';
	import { i18n } from '#lib/i18n.svelte.ts';
	import { iconOf, KINDS } from '#lib/kinds.ts';
	import { Loader } from '#lib/remote.svelte.ts';

	const ALL_KINDS = [...KINDS.map((k) => k.kind as string), 'event'];
	// Events are many (a lunch every week): shown when the person asks for them.
	const DEFAULT_KINDS = KINDS.map((k) => k.kind as string);
	const graph = new Loader<GraphView>();

	const id = $derived(page.url.searchParams.get('id') ?? undefined);
	const depth = $derived(page.url.searchParams.get('depth') === '1' ? 1 : 2);
	const kinds = $derived(page.url.searchParams.get('kinds')?.split(',').filter(Boolean) ?? DEFAULT_KINDS);
	const hidden = $derived(page.url.searchParams.get('hide')?.split(',').filter(Boolean) ?? []);

	// The categories the person turned off; the others are shown.
	let categoryNames: string[] = $state([]);

	$effect(() => {
		const categories = hidden.length ? categoryNames.filter((c) => !hidden.includes(c)).join(',') || '-' : undefined;
		const askedKinds = kinds.join(',');
		const center = id;
		const steps = depth;
		graph.load(() => api.graph(center, steps, askedKinds, categories), true);
	});

	const view = $derived(graph.state.kind === 'ready' ? graph.state.value : null);
	$effect(() => {
		if (view && !categoryNames.length) categoryNames = view.categories.map((c) => c.name);
	});
	const center = $derived(view?.nodes[0] ?? null);

	/** The address of this screen with some of its choices changed. */
	function address(change: Record<string, string | null>): string {
		const url = new URL(page.url.href);
		for (const [key, value] of Object.entries(change)) {
			if (value === null) url.searchParams.delete(key);
			else url.searchParams.set(key, value);
		}
		return `${url.pathname}${url.search}`;
	}

	function recenter(node: NodeView) {
		goto(address({ id: node.id }), { reset: false });
	}

	function toggleKind(kind: string) {
		const next = kinds.includes(kind) ? kinds.filter((k) => k !== kind) : ALL_KINDS.filter((k) => k === kind || kinds.includes(k));
		const usual = next.length === DEFAULT_KINDS.length && DEFAULT_KINDS.every((k) => next.includes(k));
		goto(address({ kinds: usual ? null : next.join(',') }), { reset: false, replace: true });
	}

	function toggleCategory(name: string) {
		const next = hidden.includes(name) ? hidden.filter((c) => c !== name) : [...hidden, name];
		goto(address({ hide: next.length ? next.join(',') : null }), { reset: false, replace: true });
	}

	// One movement when the graph changes: every file glides from where it was (a new
	// one from the file it hangs on) to its new place.
	const reduced = typeof matchMedia === 'function' && matchMedia('(prefers-reduced-motion: reduce)').matches;
	const progress = new Tween(1, { duration: reduced ? 0 : 480, easing: cubicOut });
	let from: Map<string, { x: number; y: number }> = new Map();
	let spots: Spot[] = $state([]);
	let shownGraph = $state<GraphView | null>(null);
	$effect(() => {
		if (!view || view === shownGraph) return;
		const before = new Map<string, { x: number; y: number }>();
		if (shownGraph) {
			const t = progress.current;
			shownGraph.nodes.forEach((n, i) => {
				const a = from.get(n.id) ?? spots[i];
				const b = spots[i];
				if (a && b) before.set(n.id, { x: a.x + (b.x - a.x) * t, y: a.y + (b.y - a.y) * t });
			});
		}
		from = before;
		spots = layout(view);
		shownGraph = view;
		progress.set(0, { duration: 0 });
		progress.set(1);
	});

	const at = (i: number) => {
		const node = shownGraph?.nodes[i];
		const end = spots[i];
		if (!node || !end) return { x: WIDTH / 2, y: HEIGHT / 2 };
		const start = from.get(node.id) ?? from.get(shownGraph?.nodes[0]?.id ?? '') ?? end;
		const t = progress.current;
		return { x: start.x + (end.x - start.x) * t, y: start.y + (end.y - start.y) * t };
	};

	let lit = $state<number | null>(null);
	const counts = $derived(shownGraph ? shownGraph.nodes.filter((n) => n.depth === 1).length : 0);
	const labelled = (node: NodeView) =>
		node.depth === 0 || (node.depth === 1 && counts <= 36) || (shownGraph?.nodes.length ?? 0) <= 30;
	const short = (name: string) => (name.length > 24 ? `${name.slice(0, 23)}…` : name);
	/** A file's name; an event's with its day, since many share a title. */
	const title = (node: NodeView) => (node.start ? `${node.name}, ${shortDay(dayOf(node.start))}` : node.name);
	const radius = (depth: number) => (depth === 0 ? 30 : depth === 1 ? 17 : 10);

	/** The words of the links between two files, for the list. */
	function between(a: number, b: number): string[] {
		if (!view) return [];
		return view.edges.filter((e) => (e.from === a && e.to === b) || (e.from === b && e.to === a)).flatMap((e) => (e.word ? [e.word] : []));
	}

	const firstStep = $derived(view ? view.nodes.map((n, i) => ({ n, i })).filter(({ n }) => n.depth === 1) : []);
	const secondStep = $derived(view ? view.nodes.map((n, i) => ({ n, i })).filter(({ n }) => n.depth === 2) : []);
	const viaOf = (i: number) =>
		firstStep.filter(({ i: j }) => view?.edges.some((e) => (e.from === i && e.to === j) || (e.from === j && e.to === i))).map(({ n }) => n.name);
</script>

<svelte:head><title>{i18n.t.graph}{center ? ` · ${center.name}` : ''} · Ipse</title></svelte:head>

<div class="screen graph-screen">
	<header class="page-head">
		<h1>{i18n.t.graph}</h1>
		{#if center}<p class="lead">{i18n.t.graphAround(center.name)}</p>{/if}
	</header>

	<div class="graph-filters">
		<fieldset>
			<legend>{i18n.t.graphDistance}</legend>
			<div class="segmented">
				<button type="button" aria-pressed={depth === 1} onclick={() => goto(address({ depth: '1' }), { reset: false, replace: true })}
					>{i18n.t.oneStep}</button
				>
				<button type="button" aria-pressed={depth === 2} onclick={() => goto(address({ depth: null }), { reset: false, replace: true })}
					>{i18n.t.twoSteps}</button
				>
			</div>
		</fieldset>
		<fieldset>
			<legend>{i18n.t.graphKinds}</legend>
			<div class="toggles">
				{#each ALL_KINDS as kind (kind)}
					<button
						type="button"
						class="toggle"
						aria-pressed={kinds.includes(kind)}
						disabled={kinds.length === 1 && kinds.includes(kind)}
						onclick={() => toggleKind(kind)}
						><svg viewBox="0 0 24 24" width="15" height="15" aria-hidden="true"><path d={iconOf(kind)} /></svg>{i18n.t.kinds[kind]}</button
					>
				{/each}
			</div>
		</fieldset>
		{#if view}
			<fieldset>
				<legend>{i18n.t.graphCategories}</legend>
				<div class="toggles">
					{#each view.categories as category (category.name)}
						<button
							type="button"
							class="toggle"
							class:sensitive={category.sensitive}
							aria-pressed={!hidden.includes(category.name)}
							disabled={!hidden.includes(category.name) && hidden.length >= view.categories.length - 1}
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
		{/if}
	</div>

	{#if shownGraph}
		<figure class="graph" aria-label={center ? i18n.t.graphLabel(center.name) : i18n.t.graph}>
			<svg viewBox="0 0 {WIDTH} {HEIGHT}" aria-hidden="true" data-testid="graph">
				<g class="edges">
					{#each shownGraph.edges as edge, k (k)}
						{@const a = at(edge.from)}
						{@const b = at(edge.to)}
						<line
							x1={a.x}
							y1={a.y}
							x2={b.x}
							y2={b.y}
							class:core={edge.from === 0 || edge.to === 0}
							class:lit={lit !== null && (edge.from === lit || edge.to === lit)}
						/>
					{/each}
				</g>
				{#each shownGraph.nodes as node, i (node.id)}
					{@const p = at(i)}
					{@const side = labelSide(spots[i] ?? { x: 0, y: 0, angle: 0 }, node.depth)}
					{@const r = radius(node.depth)}
					<g
						class="node depth-{node.depth}"
						class:person={node.kind === 'person'}
						class:sensitive={node.sensitive}
						class:labelled={labelled(node)}
						class:lit={lit === i}
						transform="translate({p.x} {p.y})"
						onclick={() => (node.depth === 0 ? goto(hrefOf({ id: node.id, kind: node.kind }) ?? '/memory') : recenter(node))}
						onpointerenter={() => (lit = i)}
						onpointerleave={() => (lit = null)}
						role="presentation"
					>
						<circle {r} />
						{#if node.depth < 2}
							{@const size = node.depth === 0 ? 28 : 16}
							<svg x={-size / 2} y={-size / 2} width={size} height={size} viewBox="0 0 24 24" class="icon"><path d={iconOf(node.kind)} /></svg>
						{/if}
						<text x={side.dx} y={side.dy} text-anchor={side.anchor}>{short(node.name)}{#if node.start && node.depth > 0}<tspan class="day">{` ${shortDay(dayOf(node.start))}`}</tspan>{/if}</text>
					</g>
				{/each}
			</svg>
			<figcaption>
				<span>{i18n.t.graphHint}</span>
				{#if shownGraph.nodes.some((n) => n.sensitive)}<span>{i18n.t.graphDashed}</span>{/if}
			</figcaption>
		</figure>
		{#if shownGraph.more}<p class="quiet">{i18n.t.graphMore(shownGraph.more)}</p>{/if}
		{#if shownGraph.nodes.length === 1}<p class="lead">{i18n.t.graphAlone}</p>{/if}
	{:else if graph.state.kind === 'missing'}
		<h2>{i18n.t.notFound}</h2>
	{:else if graph.state.kind === 'failed'}
		<p class="quiet">{i18n.t.failed}</p>
	{/if}

	{#if view && center}
		<section aria-labelledby="as-list" class="graph-list">
			<h2 id="as-list">{i18n.t.graphAsList}</h2>
			<p>
				<a href={hrefOf({ id: center.id, kind: center.kind })}>{title(center)}</a>
				<span class="quiet">· {i18n.t.kind[center.kind]}</span>
			</p>
			{#each [{ title: i18n.t.oneStepAway, items: firstStep }, { title: i18n.t.twoStepsAway, items: secondStep }] as group (group.title)}
				{#if group.items.length}
					<h3>{group.title}</h3>
					<ul data-testid="graph-list">
						{#each group.items as { n, i } (n.id)}
							{@const words = n.depth === 1 ? between(0, i) : []}
							<li>
								<a href={hrefOf({ id: n.id, kind: n.kind })}>{title(n)}</a>
								<span class="quiet"
									>{i18n.t.kind[n.kind]}{#if words.length}, {words.join(', ')}{/if}{#if n.depth === 2 && viaOf(i).length}, {i18n.t.via(viaOf(i).join(', '))}{/if}</span
								>
								{#if n.sensitive}<span class="visually-hidden">, {i18n.t.hiddenFromAis}</span>{/if}
								<button type="button" class="link-button" aria-label={i18n.t.recenter(title(n))} onclick={() => recenter(n)}>{i18n.t.recenterShort}</button>
							</li>
						{/each}
					</ul>
				{/if}
			{/each}
		</section>
	{/if}
</div>
