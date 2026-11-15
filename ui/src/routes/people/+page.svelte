<script lang="ts">
	import { onMount } from 'svelte';
	import { page } from '$app/state';
	import { replaceState } from '$app/navigation';
	import { api, type PeopleView } from '#lib/api.ts';
	import Face from '#lib/Face.svelte';
	import { i18n } from '#lib/i18n.svelte.ts';
	import { icons } from '#lib/icons.ts';
	import { Loader } from '#lib/remote.svelte.ts';

	let query = $state(page.url.searchParams.get('q') ?? '');
	const people = new Loader<PeopleView>();
	let extra: PeopleView['items'] = $state([]);
	let timer: ReturnType<typeof setTimeout> | undefined;

	function search(keep: boolean) {
		extra = [];
		const q = query.trim();
		people.load(() => api.people(q), keep);
		const url = new URL(page.url.href);
		if (q) url.searchParams.set('q', q);
		else url.searchParams.delete('q');
		replaceState(url, {});
	}

	function typed() {
		clearTimeout(timer);
		timer = setTimeout(() => search(true), 120);
	}

	async function more(already: number) {
		const next = await api.people(query.trim(), already);
		extra = [...extra, ...next.items];
		if (people.state.kind === 'ready') people.state.value.more = next.more;
	}

	onMount(() => search(false));
</script>

<div class="screen">
	<header class="page-head">
		<h1>{i18n.t.people}</h1>
		{#if people.state.kind === 'ready' && !query.trim()}
			<p class="date">{i18n.t.peopleCount(people.state.value.total)}</p>
		{/if}
	</header>

	<div class="search">
		<label for="search">{i18n.t.search}</label>
		<div class="field">
			<svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true"><path d={icons.search} /></svg>
			<input
				id="search"
				type="search"
				autocomplete="off"
				spellcheck="false"
				placeholder={i18n.t.searchHint}
				bind:value={query}
				oninput={typed}
			/>
		</div>
	</div>

	{#if people.state.kind === 'ready'}
		{@const view = people.state.value}
		{@const items = [...view.items, ...extra]}
		{#if query.trim()}<p class="date" aria-live="polite">{i18n.t.found(view.total)}</p>{/if}
		{#if items.length}
			<ul class="people" data-testid="people">
				{#each items as person (person.id)}
					<li>
						<a href="/people/{person.id}">
							<Face name={person.name} size={34} />
							<span class="who">
								<span class="name">{person.name}</span>
								{#if person.aliases.length}<span class="aliases">{person.aliases.join(', ')}</span>{/if}
							</span>
							{#if person.sensitive}
								<svg class="lock" viewBox="0 0 24 24" width="16" height="16" role="img" aria-label={i18n.t.hiddenFromAis}
									><rect x="5" y="10.5" width="14" height="10" rx="2.5" /><path d="M8.5 10.5V7.5a3.5 3.5 0 0 1 7 0v3" /></svg
								>
							{/if}
						</a>
					</li>
				{/each}
			</ul>
			{#if view.more}
				<button type="button" class="more" onclick={() => more(items.length)}>{i18n.t.showMore}</button>
			{/if}
		{:else if query.trim()}
			<p class="lead">{i18n.t.nobodyFound}</p>
		{:else}
			<div class="empty-thread">
				<p class="lead">{i18n.t.nobodyYet}</p>
				<p>{i18n.t.nobodyYetHint}</p>
			</div>
		{/if}
	{:else if people.state.kind === 'failed'}
		<p class="quiet">{i18n.t.failed}</p>
	{/if}
</div>
