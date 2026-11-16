<script lang="ts">
	import { page } from '$app/state';
	import { goto } from '$app/navigation';
	import { tick } from 'svelte';
	import { api, SessionEnded, type Draft, type EditView, type EntityView, type FileView } from '#lib/api.ts';
	import { app } from '#lib/app.svelte.ts';
	import Confirm, { type Question } from '#lib/Confirm.svelte';
	import Editor from '#lib/Editor.svelte';
	import Events from '#lib/Events.svelte';
	import Notice from '#lib/Notice.svelte';
	import Face from '#lib/Face.svelte';
	import { birthday, dayOf, phone as formatPhone, fromSources } from '#lib/format.ts';
	import { i18n } from '#lib/i18n.svelte.ts';
	import { folderOf, iconOf } from '#lib/kinds.ts';
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
	const person = $derived(entity?.kind === 'person');

	let form: EditView | null = $state(null);
	let outcome: { tone: 'done' | 'warn'; text: string } | null = $state(null);
	let question: Question | null = $state(null);
	let editButton: HTMLButtonElement | undefined = $state();

	async function edit() {
		outcome = null;
		try {
			form = await api.editForm(page.params.id ?? '');
			await tick();
			document.getElementById('edit-name')?.focus();
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
		if (!form || !entity) return;
		const asked = form;
		const names = [...new Set(asked.links.map((l) => l.name))];
		const yes = await new Promise<boolean>((resolve) => {
			question = {
				title: i18n.t.deleteTitle(entity?.name ?? ''),
				body: (names.length ? i18n.t.deleteLinked(names) : '') + i18n.t.deleteUndo,
				yes: i18n.t.deleteYes,
				no: i18n.t.deleteNo,
				resolve
			};
		});
		if (!yes) return;
		try {
			await api.deleteFile(asked.id, asked.version);
			await goto(`/memory/${folderOf(entity.kind)}`);
		} catch (error) {
			if (error instanceof SessionEnded) app.state = { kind: 'ended' };
			outcome = { tone: 'warn', text: i18n.t.changedMeanwhile };
		}
	}

	/** The name a missing file's link suggests: `paul-roux` reads Paul Roux. */
	const nameOf = (shown: string, stem: string) =>
		shown !== stem ? shown : stem.split('-').map((w) => w.charAt(0).toLocaleUpperCase() + w.slice(1)).join(' ');
</script>

<svelte:head>{#if entity}<title>{entity.name} · Ipse</title>{/if}</svelte:head>

{#if entity}
	<article class="screen">
		<p class="crumbs">
			<a href="/memory">{i18n.t.memory}</a>
			<span aria-hidden="true">/</span>
			<a href="/memory/{folderOf(entity.kind)}">{i18n.t.kinds[entity.kind] ?? entity.kind}</a>
		</p>
		<header class="person-head">
			{#if person}
				<Face name={entity.name} size={72} />
			{:else}
				<span class="kind-icon big" aria-hidden="true"
					><svg viewBox="0 0 24 24" width="34" height="34"><path d={iconOf(entity.kind)} /></svg></span
				>
			{/if}
			<div>
				<h1>{entity.name}</h1>
				{#if entity.aliases.length}<p class="date">{i18n.t.alsoCalled(entity.aliases.join(', '))}</p>{/if}
			</div>
			{#if !form}
				<button type="button" class="button small edit" bind:this={editButton} onclick={edit}>{i18n.t.editFile}</button>
			{/if}
		</header>

		<div class="live" role="status" aria-live="polite">
			{#if outcome}<Notice tone={outcome.tone}>{outcome.text}</Notice>{/if}
		</div>

		{#if form}
			<section aria-label={i18n.t.editTitle(entity.name)}>
				<Editor {form} {save} cancel={() => close(null)} />
				{#if !form.owner}
					<button type="button" class="link-button danger" onclick={remove}>{i18n.t.deleteFile}</button>
				{/if}
			</section>
			<Confirm bind:question />
		{:else}

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
						<dd>
							<Mention link={relation.to} />
							{#if relation.missing}
								<a
									class="create-missing"
									href="/memory/new?name={encodeURIComponent(nameOf(relation.to.name, relation.missing))}&stem={encodeURIComponent(relation.missing)}"
									aria-label={i18n.t.createMissingLabel(nameOf(relation.to.name, relation.missing))}>{i18n.t.createMissing}</a
								>
							{/if}
						</dd>
					{/each}
				</dl>
			</section>
		{/if}

		{#if entity.backlinks.length}
			<section aria-labelledby="backlinks">
				<h2 id="backlinks">{i18n.t.linksHere}</h2>
				<dl class="relations" data-testid="backlinks">
					{#each entity.backlinks as link, i (i)}
						<dt><Mention link={link.from} /></dt>
						<dd>{link.word ?? ''}</dd>
					{/each}
				</dl>
			</section>
		{/if}

		{#if !entity.owner}
			<section aria-labelledby="thread">
				<h2 id="thread">{person ? i18n.t.onTheThread : i18n.t.eventsHere}</h2>
				{#if entity.events.length}
					<Events events={entity.events} {today} />
				{:else}
					<p class="quiet">{person ? i18n.t.noEvents : i18n.t.noEventsHere}</p>
				{/if}
			</section>
		{/if}

		{#if entity.notes}
			<section aria-labelledby="notes">
				<h2 id="notes">{i18n.t.notes}</h2>
				<p class="notes">{entity.notes}</p>
			</section>
		{/if}

		{#if entity.sources.length}<p class="source">{fromSources(entity.sources)}</p>{/if}
		{/if}
	</article>
{:else if file.state.kind === 'missing' || (file.state.kind === 'ready' && !entity)}
	<div class="screen"><h1>{i18n.t.notFound}</h1><p><a href="/memory">{i18n.t.memory}</a></p></div>
{:else if file.state.kind === 'failed'}
	<p class="quiet">{i18n.t.failed}</p>
{/if}
