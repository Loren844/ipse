<script lang="ts">
	// A new file of the memory, made by the person: from a section's "Add", or from a
	// link to a file that does not exist yet (its name then comes with it).
	import { page } from '$app/state';
	import { goto } from '$app/navigation';
	import { api, type Draft } from '#lib/api.ts';
	import Editor, { blank } from '#lib/Editor.svelte';
	import { hrefOf } from '#lib/format.ts';
	import { i18n } from '#lib/i18n.svelte.ts';
	import { KINDS } from '#lib/kinds.ts';

	const kinds = [...KINDS.map((k) => k.kind as string), 'event'];
	const asked = page.url.searchParams.get('kind');
	const name = page.url.searchParams.get('name') ?? '';
	const stem = page.url.searchParams.get('stem') ?? undefined;
	let kind = $state(asked && kinds.includes(asked) ? asked : 'person');
	const fixed = !!asked && kinds.includes(asked);
	// A link names an entity, never an event.
	const choices = stem ? kinds.filter((k) => k !== 'event') : kinds;

	async function save(draft: Draft) {
		const saved = await api.createFile(kind, draft, stem);
		await goto(hrefOf({ id: saved.id, kind: saved.kind }) ?? '/memory');
	}

	function cancel() {
		history.back();
	}
</script>

<svelte:head><title>{i18n.t.addKind[kind]} · Ipse</title></svelte:head>

<div class="screen">
	<header class="page-head">
		<p class="crumbs"><a href="/memory">{i18n.t.memory}</a></p>
		<h1>{i18n.t.addKind[kind]}</h1>
	</header>
	{#if !fixed}
		<fieldset class="kind-choice">
			<legend>{i18n.t.kindOfFile}</legend>
			<div class="segmented">
				{#each choices as choice (choice)}
					<button type="button" aria-pressed={kind === choice} onclick={() => (kind = choice)}>{i18n.t.kind[choice]}</button>
				{/each}
			</div>
		</fieldset>
	{/if}
	{#key kind}
		<Editor form={blank(kind, name)} creating {save} {cancel} />
	{/key}
</div>
