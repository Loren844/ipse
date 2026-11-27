<script lang="ts">
	// The map (issues #135 and #212): places as circles sized by visits, close ones in
	// groups that open with a click, drawn by MapLibre: smooth moving and zooming, with
	// the mouse, a trackpad, a finger or the keyboard. Without the base map nothing is
	// fetched; with the person's consent, OpenFreeMap's tiles are drawn under the places,
	// light or dark with the interface.
	import { onMount, untrack } from 'svelte';
	import type { GeoJSONSource, LngLatBoundsLike, Map as MapLibre, MapMouseEvent } from 'maplibre-gl';
	import 'maplibre-gl/dist/maplibre-gl.css';
	import type { MapPlaceView } from './api.ts';
	import { i18n } from './i18n.svelte.ts';
	import { mapStyle } from './mapStyle.ts';
	// MapLibre's build that runs under a strict Content-Security-Policy, and its worker:
	// two files of the interface, loaded only on this screen, never at start.
	import library from 'maplibre-gl/dist/maplibre-gl-csp.js?url';
	import worker from 'maplibre-gl/dist/maplibre-gl-csp-worker.js?url';

	type MapLibreGl = typeof import('maplibre-gl');
	let loading: Promise<MapLibreGl> | null = null;

	/** Loads MapLibre once, as a script of the interface. */
	function maplibre(): Promise<MapLibreGl> {
		const ready = (window as unknown as { maplibregl?: MapLibreGl }).maplibregl;
		if (ready) return Promise.resolve(ready);
		loading ??= new Promise((resolve, reject) => {
			const script = document.createElement('script');
			script.src = library;
			script.onload = () => {
				const lib = (window as unknown as { maplibregl?: MapLibreGl }).maplibregl;
				if (lib) {
					lib.setWorkerUrl(worker);
					resolve(lib);
				} else reject(new Error('MapLibre did not load.'));
			};
			script.onerror = () => {
				loading = null;
				reject(new Error('MapLibre did not load.'));
			};
			document.head.append(script);
		});
		return loading;
	}

	let {
		places,
		selected = $bindable(null),
		tiles
	}: { places: MapPlaceView[]; selected: string | null; tiles: boolean } = $props();

	let box: HTMLDivElement | undefined = $state();
	let map: MapLibre | null = $state(null);
	let failed = $state(false);

	/** Draws the map in `style`, and says its ground's colour on the element. */
	function draw(target: MapLibre, style: ReturnType<typeof mapStyle>) {
		target.setStyle(style);
		const ground = style.layers[0];
		if (box && ground.type === 'background') box.dataset.ground = String(ground.paint?.['background-color'] ?? '');
	}

	/** All the places, with a margin. */
	function bounds(): LngLatBoundsLike | undefined {
		if (!places.length) return undefined;
		const lons = places.map((p) => p.lon);
		const lats = places.map((p) => p.lat);
		return [
			[Math.min(...lons), Math.min(...lats)],
			[Math.max(...lons), Math.max(...lats)]
		];
	}

	function fit() {
		const all = bounds();
		if (map && all) map.fitBounds(all, { padding: 64, maxZoom: 15, duration: matchMedia('(prefers-reduced-motion: reduce)').matches ? 0 : 400 });
	}

	onMount(() => {
		let gone = false;
		let theme: MutationObserver | undefined;
		(async () => {
			let lib: typeof import('maplibre-gl');
			try {
				lib = await maplibre();
			} catch {
				failed = true;
				return;
			}
			if (gone || !box) return;
			let made: MapLibre;
			try {
				made = new lib.Map({
					container: box,
					style: mapStyle(places, selected, tiles),
					bounds: bounds(),
					fitBoundsOptions: { padding: 64, maxZoom: 15 },
					maxZoom: 18,
					dragRotate: false,
					pitchWithRotate: false,
					touchPitch: false,
					attributionControl: false,
					fadeDuration: 0
				});
			} catch {
				failed = true;
				return;
			}
			made.touchZoomRotate.disableRotation();
			made.keyboard.disableRotation();
			made.getCanvas().setAttribute('aria-label', i18n.t.mapLabel);
			made.on('click', 'places', (e: MapMouseEvent & { features?: { properties: Record<string, unknown> }[] }) => {
				const id = e.features?.[0]?.properties.id;
				if (typeof id === 'string') selected = id;
			});
			made.on('click', 'groups', async (e: MapMouseEvent & { features?: { properties: Record<string, unknown>; geometry: import('geojson').Geometry }[] }) => {
				const group = e.features?.[0];
				const source = made.getSource('places') as GeoJSONSource | undefined;
				if (!group || !source || group.geometry.type !== 'Point') return;
				const zoom = await source.getClusterExpansionZoom(group.properties.cluster_id as number);
				made.easeTo({ center: group.geometry.coordinates as [number, number], zoom });
			});
			for (const layer of ['places', 'groups']) {
				made.on('mouseenter', layer, () => (made.getCanvas().style.cursor = 'pointer'));
				made.on('mouseleave', layer, () => (made.getCanvas().style.cursor = ''));
			}
			// What is drawn, said on the element: how many places and groups, and where the
			// first place is, for tests and tools that cannot see the canvas.
			made.on('idle', () => {
				if (!box) return;
				const drawn = made.queryRenderedFeatures({ layers: ['places', 'groups'] });
				box.dataset.drawn = String(drawn.length);
				const first = drawn.find((f) => f.layer.id === 'places' && f.geometry.type === 'Point');
				if (first?.geometry.type === 'Point') {
					const at = made.project(first.geometry.coordinates as [number, number]);
					box.dataset.first = `${Math.round(at.x)},${Math.round(at.y)},${first.properties.id}`;
				}
			});
			map = made;
			draw(made, mapStyle(places, selected, tiles));
			// The frame has its size once laid out: fit the places in it then.
			made.once('load', () => {
				made.resize();
				const all = bounds();
				if (all) made.fitBounds(all, { padding: 64, maxZoom: 15, duration: 0 });
			});
			// Light or dark with the interface.
			theme = new MutationObserver(() => draw(made, mapStyle(places, selected, tiles)));
			theme.observe(document.documentElement, { attributes: true, attributeFilter: ['data-theme'] });
		})();
		return () => {
			gone = true;
			theme?.disconnect();
			map?.remove();
			map = null;
		};
	});

	// The base map turned on or off, or the chosen place: the same style, drawn again.
	$effect(() => {
		const style = mapStyle(places, selected, tiles);
		untrack(() => map && draw(map, style));
	});

	// The chosen place comes into view, without zooming out.
	$effect(() => {
		const id = selected;
		untrack(() => {
			const place = places.find((p) => p.id === id);
			if (map && place) map.easeTo({ center: [place.lon, place.lat], zoom: Math.max(map.getZoom(), 13) });
		});
	});
</script>

<div class="map-frame">
	<div class="map" bind:this={box} role="application" aria-label={i18n.t.mapLabel} data-places={places.length}></div>
	{#if failed}
		<p class="map-failed quiet">{i18n.t.mapNoWebgl}</p>
	{/if}
	<div class="map-zoom">
		<button type="button" class="button small" aria-label={i18n.t.zoomIn} onclick={() => map?.zoomIn()}>+</button>
		<button type="button" class="button small" aria-label={i18n.t.zoomOut} onclick={() => map?.zoomOut()}>−</button>
		<button type="button" class="button small" onclick={fit}>{i18n.t.mapAll}</button>
	</div>
	{#if tiles}
		<p class="map-credit">
			© <a href="https://www.openstreetmap.org/copyright" target="_blank" rel="noreferrer">{i18n.t.osmContributors}</a>,
			<a href="https://openfreemap.org" target="_blank" rel="noreferrer">OpenFreeMap</a>
		</p>
	{/if}
</div>
