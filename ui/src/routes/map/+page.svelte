<script lang="ts">
	// Where one has been (issue #135): the places with a position on a map, sized by
	// how often one went; a place opens its visits and the people there.
	import { api, type EntityView, type MapView } from '#lib/api.ts';
	import { dayOf, longDay, timeOf } from '#lib/format.ts';
	import { i18n } from '#lib/i18n.svelte.ts';
	import Mention from '#lib/Mention.svelte';
	import Notice from '#lib/Notice.svelte';
	import PlacesMap from '#lib/PlacesMap.svelte';
	import { Loader } from '#lib/remote.svelte.ts';
	import Sensitive from '#lib/Sensitive.svelte';

	const TILES_KEY = 'ipse-map-tiles';
	const remote = new Loader<MapView>();
	remote.load(() => api.map());
	const view = $derived(remote.state.kind === 'ready' ? remote.state.value : null);
	let selected: string | null = $state(null);
	const place = $derived(view?.places.find((p) => p.id === selected) ?? null);
	const today = dayOf(new Date().toISOString());
	const closed = $derived(view?.places.find((p) => p.sensitive)?.category ?? null);

	/** Tiles only once the person said yes, in this browser. */
	let tiles = $state(
		(() => {
			try {
				return localStorage.getItem(TILES_KEY) === 'osm';
			} catch {
				return false;
			}
		})()
	);
	function useTiles(yes: boolean) {
		tiles = yes;
		try {
			if (yes) localStorage.setItem(TILES_KEY, 'osm');
			else localStorage.removeItem(TILES_KEY);
		} catch {
			// The choice then lasts until the page is closed.
		}
	}

	// The chosen place's visits, from its page.
	let visits: EntityView['events'] = $state([]);
	$effect(() => {
		const id = selected;
		visits = [];
		if (!id) return;
		api
			.file(id)
			.then((file) => {
				if (selected === id && file.type === 'entity') visits = (file.value as EntityView).events.slice(0, 12);
			})
			.catch(() => {});
	});
</script>

<svelte:head><title>{i18n.t.map} · Ipse</title></svelte:head>

<div class="screen wide map-screen">
	<header class="page-head">
		<h1>{i18n.t.map}</h1>
		<p class="intro">{i18n.t.mapLead}</p>
	</header>

	{#if remote.state.kind === 'failed'}
		<p class="quiet">{i18n.t.failed}</p>
	{:else if view && !view.places.length}
		<p class="quiet">{i18n.t.mapEmpty}</p>
	{:else if view}
		{#if closed}<Sensitive category={closed} />{/if}
		<div class="tiles-choice">
			{#if tiles}
				<p class="quiet">{i18n.t.tilesOn}</p>
				<button type="button" class="button small" onclick={() => useTiles(false)}>{i18n.t.tilesHide}</button>
			{:else}
				<Notice tone="warn">{i18n.t.tilesOffer}</Notice>
				<button type="button" class="button small" onclick={() => useTiles(true)}>{i18n.t.tilesShow}</button>
			{/if}
		</div>

		<div class="map-layout">
			<PlacesMap places={view.places} bind:selected {tiles} />
			<aside class="map-side" aria-live="polite">
				{#if place}
					<h2><a href="/memory/places/{place.id}">{place.name}</a></h2>
					<p class="quiet">
						{place.visits ? i18n.t.visitsCount(place.visits) : i18n.t.noVisits}{#if place.last}, {i18n.t.lastVisit(longDay(dayOf(place.last), today))}{/if}
					</p>
					{#if place.sensitive}<Sensitive category={place.category} />{/if}
					{#if place.people.length}
						<h3>{i18n.t.seenThere}</h3>
						<p class="seen">
							{#each place.people as met, i (i)}<Mention link={met.person} /><span class="seen-count">{met.events}</span>{/each}
						</p>
					{/if}
					{#if visits.length}
						<h3>{i18n.t.visitsThere}</h3>
						<ul class="visits">
							{#each visits as event (event.id)}
								<li><a href="/agenda/{event.id}">{event.title}</a> <span class="quiet">{longDay(dayOf(event.start), today)}, {timeOf(event.start)}</span></li>
							{/each}
						</ul>
					{/if}
				{:else}
					<p class="quiet">{i18n.t.mapPick}</p>
				{/if}
			</aside>
		</div>

		<section aria-labelledby="all-places">
			<h2 id="all-places">{i18n.t.placesByVisits}</h2>
			<ol class="map-list">
				{#each view.places as p (p.id)}
					<li>
						<button type="button" class="link-button" aria-pressed={selected === p.id} onclick={() => (selected = p.id)}>{p.name}</button>
						<span class="quiet">{p.visits ? i18n.t.visitsCount(p.visits) : i18n.t.noVisits}</span>
					</li>
				{/each}
			</ol>
			{#if view.without_position}<p class="quiet">{i18n.t.withoutPosition(view.without_position)}</p>{/if}
		</section>
	{/if}
</div>
