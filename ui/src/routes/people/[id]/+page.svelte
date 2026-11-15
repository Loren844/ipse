<script lang="ts">
	import { page } from '$app/state';
	import { api, type EntityView, type FileView } from '#lib/api.ts';
	import Events from '#lib/Events.svelte';
	import Face from '#lib/Face.svelte';
	import { birthday, dayOf, phone as formatPhone, sources } from '#lib/format.ts';
	import { i18n } from '#lib/i18n.svelte.ts';
	import Mention from '#lib/Mention.svelte';
	import { Loader } from '#lib/remote.svelte.ts';
	import Sensitive from '#lib/Sensitive.svelte';

	const file = new Loader<FileView>();
	$effect(() => {
		const id = page.params.id;
		if (id) file.load(() => api.file(id));
	});
	const today = dayOf(new Date().toISOString());
	const entity = $derived(file.state.kind === 'ready' && file.state.value.type === 'entity' ? (file.state.value.value as EntityView) : null);
</script>

<svelte:head>{#if entity}<title>{entity.name} · Ipse</title>{/if}</svelte:head>

{#if entity}
	<article class="screen">
		<header class="person-head">
			<Face name={entity.name} size={72} />
			<div>
				<h1>{entity.name}</h1>
				{#if entity.aliases.length}<p class="date">{i18n.t.alsoCalled(entity.aliases.join(', '))}</p>{/if}
			</div>
		</header>

		{#if entity.sensitive}<Sensitive category={entity.category} />{/if}

		{#if entity.phones.length || entity.emails.length || entity.birthday || entity.address}
			<dl class="props">
				{#each entity.phones as phone, i (i)}<dt>{i18n.t.phone}</dt><dd><a href="tel:{phone}">{formatPhone(phone)}</a></dd>{/each}
				{#each entity.emails as email, i (i)}<dt>{i18n.t.email}</dt><dd><a href="mailto:{email}">{email}</a></dd>{/each}
				{#if entity.birthday}<dt>{i18n.t.birthday}</dt><dd>{birthday(entity.birthday)}</dd>{/if}
				{#if entity.address}<dt>{i18n.t.address}</dt><dd>{entity.address}</dd>{/if}
			</dl>
		{/if}

		{#if entity.facts.length}
			<section aria-labelledby="facts">
				<h2 id="facts">{i18n.t.facts}</h2>
				<ul class="facts">{#each entity.facts as fact, i (i)}<li>{fact}</li>{/each}</ul>
			</section>
		{/if}

		{#if entity.relations.length}
			<section aria-labelledby="relations">
				<h2 id="relations">{i18n.t.relations}</h2>
				<dl class="relations">
					{#each entity.relations as relation, i (i)}
						<dt>{relation.word}</dt>
						<dd><Mention link={relation.to} /></dd>
					{/each}
				</dl>
			</section>
		{/if}

		{#if !entity.owner}
			<section aria-labelledby="thread">
				<h2 id="thread">{i18n.t.onTheThread}</h2>
				{#if entity.events.length}
					<Events events={entity.events} {today} />
				{:else}
					<p class="quiet">{i18n.t.noEvents}</p>
				{/if}
			</section>
		{/if}

		{#if entity.notes}
			<section aria-labelledby="notes">
				<h2 id="notes">{i18n.t.notes}</h2>
				<p class="notes">{entity.notes}</p>
			</section>
		{/if}

		{#if entity.sources.length}<p class="source">{i18n.t.from} {sources(entity.sources)}.</p>{/if}
	</article>
{:else if file.state.kind === 'missing' || (file.state.kind === 'ready' && !entity)}
	<div class="screen"><h1>{i18n.t.notFound}</h1><p><a href="/people">{i18n.t.people}</a></p></div>
{:else if file.state.kind === 'failed'}
	<p class="quiet">{i18n.t.failed}</p>
{/if}
