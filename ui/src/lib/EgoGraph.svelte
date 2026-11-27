<script lang="ts">
	// The graph centred on one file (issue #213): the file in the middle, its neighbours
	// around it, a line to each, as thick as they are often together. A click on a
	// neighbour makes it the center, and the drawing glides there. The page beside it
	// holds the same neighbours as a list, for the keyboard and screen readers: the
	// drawing itself is only for the eyes.
	import { untrack } from 'svelte';
	import { Tween } from 'svelte/motion';
	import { cubicOut } from 'svelte/easing';
	import type { GraphView, NeighbourView } from './api.ts';
	import { heightFor, layoutAround, SHAPES, thickness, type Place } from './egoGraph.ts';
	import { CALM, still } from './motion.ts';

	let {
		graph,
		shown,
		centerName,
		nameOf,
		highlighted = $bindable(null),
		onchoose
	}: {
		graph: GraphView;
		shown: NeighbourView[];
		centerName: string;
		nameOf: (n: NeighbourView) => string;
		highlighted?: string | null;
		onchoose: (id: string) => void;
	} = $props();

	let width = $state(0);
	let height = $state(0);

	// Many neighbours make the drawing taller than its frame, which then scrolls.
	const tall = $derived(heightFor(shown.length, height));
	const targets = $derived(layoutAround(shown, width, tall));
	const most = $derived(Math.max(0, ...shown.map((n) => n.together)));

	// Where each file was drawn before the last change, to glide from there.
	let from = new Map<string, Place>();
	let origin: Place = { x: 0, y: 0 };
	const progress = new Tween(1);
	let drawn = new Map<string, Place>();

	const lerp = (a: Place, b: Place, t: number): Place => ({ x: a.x + (b.x - a.x) * t, y: a.y + (b.y - a.y) * t });

	$effect.pre(() => {
		// A new center, or other neighbours: start from what is drawn now.
		void graph.center.id;
		void targets;
		untrack(() => {
			from = new Map(drawn);
			origin = drawn.get(graph.center.id) ?? { x: 0, y: 0 };
			progress.set(0, { duration: 0 });
			progress.set(1, { duration: still() ? 0 : CALM, easing: cubicOut });
		});
	});

	const places = $derived.by(() => {
		const t = progress.current;
		const now = new Map<string, Place>();
		now.set(graph.center.id, lerp(from.get(graph.center.id) ?? { x: 0, y: 0 }, { x: 0, y: 0 }, t));
		for (const [id, target] of targets) now.set(id, lerp(from.get(id) ?? origin, target, t));
		return now;
	});
	$effect(() => {
		drawn = places;
	});

	/** A name short enough to sit beside its shape. */
	const short = (name: string) => (name.length > 24 ? `${name.slice(0, 23)}…` : name);
	const word = (n: NeighbourView) => n.words[0] ?? n.back[0] ?? null;
</script>

{#snippet shape(kind: string, r: number)}
	{#if SHAPES[kind] === 'square'}<rect x={-r * 0.85} y={-r * 0.85} width={r * 1.7} height={r * 1.7} />
	{:else if SHAPES[kind] === 'diamond'}<path d="M0 {-r * 1.1} {r * 1.1} 0 0 {r * 1.1} {-r * 1.1} 0Z" />
	{:else if SHAPES[kind] === 'triangle'}<path d="M0 {-r * 1.1} {r} {r * 0.75} {-r} {r * 0.75}Z" />
	{:else if SHAPES[kind] === 'down'}<path d="M0 {r * 1.1} {r} {-r * 0.75} {-r} {-r * 0.75}Z" />
	{:else if SHAPES[kind] === 'hexagon'}<path d="M{r} 0 {r / 2} {r * 0.87} {-r / 2} {r * 0.87} {-r} 0 {-r / 2} {-r * 0.87} {r / 2} {-r * 0.87}Z" />
	{:else if SHAPES[kind] === 'cross'}<path d="M{-r} {-r * 0.4}h{r * 2}v{r * 0.8}h{-r * 2}ZM{-r * 0.4} {-r}h{r * 0.8}v{r * 2}h{-r * 0.8}Z" />
	{:else if SHAPES[kind] === 'dot'}<circle r={r * 0.7} />
	{:else}<circle {r} />{/if}
{/snippet}

<div class="ego-graph" bind:clientWidth={width} bind:clientHeight={height}>
	{#if width && height}
		<svg
			data-testid="graph"
			data-center={graph.center.id}
			data-files={shown.length + 1}
			data-moving={progress.current < 1 ? 'true' : 'false'}
			viewBox="{-width / 2} {-tall / 2} {width} {tall}"
			{width}
			height={tall}
			aria-hidden="true"
		>
			{#each shown as n (n.file.id)}
				{@const at = places.get(n.file.id)}
				{#if at}
					{@const w = word(n)}
					<g
						class="ego-link"
						class:lit={highlighted === n.file.id}
						role="presentation"
						onmouseenter={() => (highlighted = n.file.id)}
						onmouseleave={() => (highlighted = null)}
					>
						<line x1="0" y1="0" x2={at.x} y2={at.y} class="hit" />
						<line x1="0" y1="0" x2={at.x} y2={at.y} stroke-width={thickness(n.together, most)} class:written={!!w} />
						{#if w}
							<text class="ego-word" x={at.x * 0.55} y={at.y * 0.55 - 4} text-anchor="middle">{w}</text>
						{/if}
					</g>
				{/if}
			{/each}
			{#each shown as n (n.file.id)}
				{@const at = places.get(n.file.id)}
				{#if at}
					<!-- svelte-ignore a11y_click_events_have_key_events -->
					<g
						class="ego-node"
						class:hollow={n.file.sensitive}
						class:lit={highlighted === n.file.id}
						data-id={n.file.id}
						transform="translate({at.x} {at.y})"
						role="presentation"
						onclick={() => onchoose(n.file.id)}
						onmouseenter={() => (highlighted = n.file.id)}
						onmouseleave={() => (highlighted = null)}
					>
						<circle class="hit" r="16" />
						{@render shape(n.file.kind, n.file.id === graph.owner ? 8 : 6)}
						<text x={at.x >= 0 ? 11 : -11} y="4" text-anchor={at.x >= 0 ? 'start' : 'end'}>{short(nameOf(n))}</text>
					</g>
				{/if}
			{/each}
			{#if places.get(graph.center.id)}
				{@const at = places.get(graph.center.id)!}
				<g class="ego-center" class:hollow={graph.center.sensitive} transform="translate({at.x} {at.y})">
					{@render shape(graph.mine ? 'person' : graph.center.kind, 11)}
					<text y="30" text-anchor="middle">{short(centerName)}</text>
				</g>
			{/if}
		</svg>
	{/if}
</div>
