<script lang="ts">
	import { page } from '$app/state';
	import { replaceState } from '$app/navigation';
	import { api, type ListView } from '#lib/api.ts';
	import Face from '#lib/Face.svelte';
	import { i18n } from '#lib/i18n.svelte.ts';
	import { icons } from '#lib/icons.ts';
	import { kindOf } from '#lib/kinds.ts';
	import { Loader } from '#lib/remote.svelte.ts';

	const kind = $derived(kindOf(page.params.folder ?? ''));
	let query = $state(page.url.searchParams.get('q') ?? '');
	const list = new Loader<ListView>();
	let extra: ListView['items'] = $state([]);
	let timer: ReturnType<typeof setTimeout> | undefined;

	function search(keep: boolean) {
		const folder = kind?.folder;
		if (!folder) return;
		extra = [];
		const q = query.trim();
		list.load(() => api.list(folder, q), keep);
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
		if (!kind) return;
		const next = await api.list(kind.folder, query.trim(), already);
		extra = [...extra, ...next.items];
		if (list.state.kind === 'ready') list.state.value.more = next.more;
	}

	// Another kind, from the same page: start again.
	$effect(() => {
		if (kind) search(false);
	});

	const title = $derived(kind ? i18n.t.kinds[kind.kind] : '');
	const label = $derived(kind?.kind === 'person' ? i18n.t.search : i18n.t.searchKind(title));
</script>

<svelte:head><title>{title} · Ipse</title></svelte:head>

{#if kind}
	<div class="screen">
		<header class="page-head">
			<p class="crumbs"><a href="/memory">{i18n.t.memory}</a></p>
			<h1>{title}</h1>
			{#if list.state.kind === 'ready' && !query.trim()}
				<p class="date">
					{kind.kind === 'person' ? i18n.t.peopleCount(list.state.value.total) : i18n.t.kindCount(list.state.value.total)}
				</p>
			{/if}
		</header>

		<div class="search">
			<label for="search">{label}</label>
			<div class="field">
				<svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true"><path d={icons.search} /></svg>
				<input
					id="search"
					type="search"
					autocomplete="off"
					spellcheck="false"
					placeholder={kind.kind === 'person' ? i18n.t.searchHint : i18n.t.searchMemoryHint}
					bind:value={query}
					oninput={typed}
				/>
			</div>
		</div>

		{#if list.state.kind === 'ready'}
			{@const view = list.state.value}
			{@const items = [...view.items, ...extra]}
			{#if query.trim()}<p class="date" aria-live="polite">{i18n.t.found(view.total)}</p>{/if}
			{#if items.length}
				<ul class="people" data-testid="people">
					{#each items as item (item.id)}
						<li>
							<a href="/memory/{kind.folder}/{item.id}">
								{#if kind.kind === 'person'}
									<Face name={item.name} size={34} />
								{:else}
									<span class="kind-icon" aria-hidden="true"
										><svg viewBox="0 0 24 24" width="18" height="18"><path d={kind.icon} /></svg></span
									>
								{/if}
								<span class="who">
									<span class="name">{item.name}</span>
									{#if item.aliases.length}<span class="aliases">{item.aliases.join(', ')}</span>{/if}
								</span>
								{#if item.sensitive}
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
				<p class="lead">{kind.kind === 'person' ? i18n.t.nobodyFound : i18n.t.nothingFound}</p>
			{:else}
				<div class="empty-thread">
					<p class="lead">{kind.kind === 'person' ? i18n.t.nobodyYet : i18n.t.nothingYet}</p>
					<p>{i18n.t.kindEmpty[kind.kind]}</p>
				</div>
			{/if}
		{:else if list.state.kind === 'failed'}
			<p class="quiet">{i18n.t.failed}</p>
		{/if}
	</div>
{:else}
	<div class="screen"><h1>{i18n.t.notFound}</h1><p><a href="/memory">{i18n.t.memory}</a></p></div>
{/if}
