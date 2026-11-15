<script lang="ts">
	import { page } from '$app/state';
	import { api, type EventView, type FileView } from '#lib/api.ts';
	import { sources, when } from '#lib/format.ts';
	import { i18n } from '#lib/i18n.svelte.ts';
	import Mention from '#lib/Mention.svelte';
	import { Loader } from '#lib/remote.svelte.ts';
	import Sensitive from '#lib/Sensitive.svelte';

	const file = new Loader<FileView>();
	$effect(() => {
		const id = page.params.id;
		if (id) file.load(() => api.file(id));
	});
	const event = $derived(file.state.kind === 'ready' && file.state.value.type === 'event' ? (file.state.value.value as EventView) : null);
</script>

<svelte:head>{#if event}<title>{event.title} · Ipse</title>{/if}</svelte:head>

{#if event}
	<article class="screen">
		<header class="page-head">
			<p class="date">
				{when(event.start)}
				{#if event.status === 'cancelled'}<span class="badge">{i18n.t.cancelled}</span>{/if}
			</p>
			<h1>{event.title}</h1>
		</header>
		{#if event.sensitive}<Sensitive category={event.category} />{/if}
		<dl class="props">
			{#if event.people.length}
				<dt>{i18n.t.with}</dt>
				<dd class="mentions">{#each event.people as person, i (i)}<Mention link={person} />{/each}</dd>
			{/if}
			{#if event.place}<dt>{i18n.t.place}</dt><dd><Mention link={event.place} /></dd>{/if}
		</dl>
		{#if event.notes}
			<section aria-labelledby="notes">
				<h2 id="notes">{i18n.t.notes}</h2>
				<p class="notes">{event.notes}</p>
			</section>
		{/if}
		{#if event.sources.length}<p class="source">{i18n.t.from} {sources(event.sources)}.</p>{/if}
	</article>
{:else if file.state.kind === 'missing' || (file.state.kind === 'ready' && !event)}
	<div class="screen"><h1>{i18n.t.notFound}</h1><p><a href="/agenda">{i18n.t.agenda}</a></p></div>
{:else if file.state.kind === 'failed'}
	<p class="quiet">{i18n.t.failed}</p>
{/if}
