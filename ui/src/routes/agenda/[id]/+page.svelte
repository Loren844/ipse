<script lang="ts">
	import { page } from '$app/state';
	import { goto } from '$app/navigation';
	import { tick } from 'svelte';
	import { api, SessionEnded, type Draft, type EditView, type EventView, type FileView } from '#lib/api.ts';
	import { app } from '#lib/app.svelte.ts';
	import Confirm, { type Question } from '#lib/Confirm.svelte';
	import Editor from '#lib/Editor.svelte';
	import { fromSources, when } from '#lib/format.ts';
	import { i18n } from '#lib/i18n.svelte.ts';
	import Mention from '#lib/Mention.svelte';
	import Notice from '#lib/Notice.svelte';
	import { Loader } from '#lib/remote.svelte.ts';
	import Sensitive from '#lib/Sensitive.svelte';

	const file = new Loader<FileView>();
	$effect(() => {
		const id = page.params.id;
		if (id) file.load(() => api.file(id));
	});
	const event = $derived(file.state.kind === 'ready' && file.state.value.type === 'event' ? (file.state.value.value as EventView) : null);

	let form: EditView | null = $state(null);
	let outcome: { tone: 'done' | 'warn'; text: string } | null = $state(null);
	let question: Question | null = $state(null);
	let editButton: HTMLButtonElement | undefined = $state();

	async function edit() {
		outcome = null;
		try {
			form = await api.editForm(page.params.id ?? '');
			await tick();
			document.getElementById('edit-title')?.focus();
		} catch (error) {
			if (error instanceof SessionEnded) app.state = { kind: 'ended' };
			outcome = { tone: 'warn', text: i18n.t.actionFailed };
		}
	}

	async function close(text: string | null) {
		form = null;
		outcome = text ? { tone: 'done', text } : null;
		await tick();
		editButton?.focus();
	}

	async function save(draft: Draft) {
		if (!form) return;
		const view = await api.saveFile(form.id, form.version, draft);
		await file.load(async () => view, true);
		await close(i18n.t.fileSaved);
	}

	async function remove() {
		if (!form || !event) return;
		const asked = form;
		const names = [...new Set(asked.links.map((l) => l.name))];
		const yes = await new Promise<boolean>((resolve) => {
			question = {
				title: i18n.t.deleteTitle(event?.title ?? ''),
				body: (names.length ? i18n.t.deleteLinked(names) : '') + i18n.t.deleteUndo,
				yes: i18n.t.deleteYes,
				no: i18n.t.deleteNo,
				resolve
			};
		});
		if (!yes) return;
		try {
			await api.deleteFile(asked.id, asked.version);
			await goto('/agenda');
		} catch (error) {
			if (error instanceof SessionEnded) app.state = { kind: 'ended' };
			outcome = { tone: 'warn', text: i18n.t.changedMeanwhile };
		}
	}
</script>

<svelte:head>{#if event}<title>{event.title} · Ipse</title>{/if}</svelte:head>

{#if event}
	<article class="screen">
		<header class="page-head">
			<p class="date">
				{when(event.start)}
				{#if event.status === 'cancelled'}<span class="badge">{i18n.t.cancelled}</span>{/if}
			</p>
			<div class="head-row">
				<h1>{event.title}</h1>
				{#if !form}
					<div class="head-actions">
						<a class="button small" href="/graph?id={event.id}" aria-label={i18n.t.inGraphLabel(event.title)}>{i18n.t.inGraph}</a>
						<button type="button" class="button small" bind:this={editButton} onclick={edit}>{i18n.t.editFile}</button>
					</div>
				{/if}
			</div>
		</header>

		<div class="live" role="status" aria-live="polite">
			{#if outcome}<Notice tone={outcome.tone}>{outcome.text}</Notice>{/if}
		</div>

		{#if form}
			<section aria-label={i18n.t.editTitle(event.title)}>
				<Editor {form} {save} cancel={() => close(null)} />
				<button type="button" class="link-button danger" onclick={remove}>{i18n.t.deleteFile}</button>
			</section>
			<Confirm bind:question />
		{:else}
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
			{#if event.sources.length}<p class="source">{fromSources(event.sources)}</p>{/if}
		{/if}
	</article>
{:else if file.state.kind === 'missing' || (file.state.kind === 'ready' && !event)}
	<div class="screen"><h1>{i18n.t.notFound}</h1><p><a href="/agenda">{i18n.t.agenda}</a></p></div>
{:else if file.state.kind === 'failed'}
	<p class="quiet">{i18n.t.failed}</p>
{/if}
