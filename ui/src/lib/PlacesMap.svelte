<script lang="ts">
	// The map (issue #135): places as circles sized by visits, drawn in Web Mercator on
	// an SVG. Without tiles it works offline (points only); with the person's consent,
	// OpenStreetMap's tiles are drawn under them. Drag to move, wheel or buttons to
	// zoom, arrow keys too.
	import { untrack } from 'svelte';
	import type { MapPlaceView } from './api.ts';
	import { i18n } from './i18n.svelte.ts';

	let {
		places,
		selected = $bindable(null),
		tiles
	}: { places: MapPlaceView[]; selected: string | null; tiles: boolean } = $props();

	/** The world at zoom 0 is 256 units wide, as tiles are. */
	const WORLD = 256;
	const MAX_ZOOM = 18;

	const project = (lat: number, lon: number) => {
		const sin = Math.sin((Math.max(-85, Math.min(85, lat)) * Math.PI) / 180);
		return {
			x: ((lon + 180) / 360) * WORLD,
			y: (0.5 - Math.log((1 + sin) / (1 - sin)) / (4 * Math.PI)) * WORLD
		};
	};

	let width = $state(800);
	let height = $state(520);
	const points = $derived(places.map((p) => ({ place: p, ...project(p.lat, p.lon) })));

	/** The part of the world shown: its left, top and width, in world units. */
	let view = $state({ x: 0, y: 0, w: WORLD });
	let fitted = $state(false);

	function fit() {
		if (!points.length) return;
		const xs = points.map((p) => p.x);
		const ys = points.map((p) => p.y);
		const [x0, x1, y0, y1] = [Math.min(...xs), Math.max(...xs), Math.min(...ys), Math.max(...ys)];
		const ratio = height / width;
		let w = Math.max(x1 - x0, (y1 - y0) / ratio) * 1.25;
		w = Math.max(w, WORLD / 2 ** 15);
		view = { x: (x0 + x1) / 2 - w / 2, y: (y0 + y1) / 2 - (w * ratio) / 2, w };
		fitted = true;
	}

	$effect(() => {
		if (!fitted && points.length && width > 0) fit();
	});

	const scale = $derived(width / view.w);
	const h = $derived(view.w * (height / width));

	function zoom(factor: number, cx = view.x + view.w / 2, cy = view.y + h / 2) {
		const w = Math.min(WORLD * 2, Math.max(WORLD / 2 ** MAX_ZOOM, view.w / factor));
		const k = w / view.w;
		view = { x: cx - (cx - view.x) * k, y: cy - (cy - view.y) * k, w };
	}

	function center(id: string) {
		const point = points.find((p) => p.place.id === id);
		if (point) view = { x: point.x - view.w / 2, y: point.y - h / 2, w: view.w };
	}

	// Centre the chosen place; what the view was is read untracked, so that moving it
	// does not centre it again.
	$effect(() => {
		const id = selected;
		if (id) untrack(() => center(id));
	});

	let drag: { x: number; y: number; vx: number; vy: number; moved: boolean } | null = null;

	function down(e: PointerEvent) {
		drag = { x: e.clientX, y: e.clientY, vx: view.x, vy: view.y, moved: false };
		(e.currentTarget as Element).setPointerCapture(e.pointerId);
	}

	function move(e: PointerEvent) {
		if (!drag) return;
		const dx = e.clientX - drag.x;
		const dy = e.clientY - drag.y;
		if (Math.abs(dx) + Math.abs(dy) > 3) drag.moved = true;
		view = { x: drag.vx - dx / scale, y: drag.vy - dy / scale, w: view.w };
	}

	function wheel(e: WheelEvent) {
		e.preventDefault();
		const box = (e.currentTarget as Element).getBoundingClientRect();
		const cx = view.x + ((e.clientX - box.left) / box.width) * view.w;
		const cy = view.y + ((e.clientY - box.top) / box.height) * h;
		zoom(e.deltaY < 0 ? 1.25 : 0.8, cx, cy);
	}

	function key(e: KeyboardEvent) {
		const step = view.w / 8;
		const moves: Record<string, [number, number]> = { ArrowLeft: [-step, 0], ArrowRight: [step, 0], ArrowUp: [0, -step], ArrowDown: [0, step] };
		if (moves[e.key]) {
			e.preventDefault();
			view = { x: view.x + moves[e.key][0], y: view.y + moves[e.key][1], w: view.w };
		} else if (e.key === '+' || e.key === '=') zoom(1.5);
		else if (e.key === '-') zoom(1 / 1.5);
	}

	/** The tiles under the view, at the zoom closest to the screen's. */
	const shownTiles = $derived.by(() => {
		if (!tiles) return [];
		const z = Math.max(0, Math.min(MAX_ZOOM, Math.round(Math.log2(scale))));
		const n = 2 ** z;
		const size = WORLD / n;
		const list: { key: string; x: number; y: number; size: number; href: string }[] = [];
		for (let tx = Math.floor(view.x / size); tx <= Math.floor((view.x + view.w) / size); tx++) {
			for (let ty = Math.max(0, Math.floor(view.y / size)); ty <= Math.min(n - 1, Math.floor((view.y + h) / size)); ty++) {
				const wrapped = ((tx % n) + n) % n;
				list.push({ key: `${z}/${tx}/${ty}`, x: tx * size, y: ty * size, size, href: `https://tile.openstreetmap.org/${z}/${wrapped}/${ty}.png` });
				if (list.length > 64) return list;
			}
		}
		return list;
	});

	const radius = (visits: number) => (5 + 2.6 * Math.sqrt(visits)) / scale;
</script>

<div class="map-frame" bind:clientWidth={width} bind:clientHeight={height}>
	<!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
	<svg
		class="map"
		class:tiles
		viewBox="{view.x} {view.y} {view.w} {h}"
		role="application"
		aria-label={i18n.t.mapLabel}
		tabindex="0"
		onpointerdown={down}
		onpointermove={move}
		onpointerup={() => (drag = null)}
		onwheel={wheel}
		onkeydown={key}
	>
		{#each shownTiles as tile (tile.key)}
			<image href={tile.href} x={tile.x} y={tile.y} width={tile.size} height={tile.size} />
		{/each}
		{#each [...points].reverse() as point (point.place.id)}
			<circle
				cx={point.x}
				cy={point.y}
				r={radius(point.place.visits)}
				class="spot"
				class:chosen={selected === point.place.id}
				role="button"
				tabindex="-1"
				aria-label={point.place.name}
				onclick={() => {
					if (!drag?.moved) selected = point.place.id;
				}}
				onkeydown={(e) => e.key === 'Enter' && (selected = point.place.id)}
			><title>{point.place.name}</title></circle>
		{/each}
	</svg>
	<div class="map-zoom">
		<button type="button" class="button small" aria-label={i18n.t.zoomIn} onclick={() => zoom(1.5)}>+</button>
		<button type="button" class="button small" aria-label={i18n.t.zoomOut} onclick={() => zoom(1 / 1.5)}>−</button>
		<button type="button" class="button small" onclick={fit}>{i18n.t.mapAll}</button>
	</div>
	{#if tiles}
		<p class="map-credit">© <a href="https://www.openstreetmap.org/copyright" target="_blank" rel="noreferrer">{i18n.t.osmContributors}</a></p>
	{/if}
</div>
