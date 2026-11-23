<script lang="ts">
	// What feeds the memory (issue #152): files imported, calendars followed by link.
	import { api, SessionEnded, type SourcesChange, type SourcesView, type SourceView } from '#lib/api.ts';
	import { app } from '#lib/app.svelte.ts';
	import Confirm, { type Question } from '#lib/Confirm.svelte';
	import { ago, runCounts } from '#lib/format.ts';
	import { i18n } from '#lib/i18n.svelte.ts';
	import Notice from '#lib/Notice.svelte';
	import { Loader } from '#lib/remote.svelte.ts';

	const screen = new Loader<SourcesView>();
	screen.load(() => api.sources());
	const view = $derived(screen.state.kind === 'ready' ? screen.state.value : null);

	let outcome: { tone: 'done' | 'warn'; text: string } | null = $state(null);
	let question: Question | null = $state(null);
	/** What is under way: an import (its file's name), following, or a source syncing. */
	let working: { kind: 'import'; name: string } | { kind: 'follow' } | { kind: 'sync'; instance: string } | null = $state(null);
	let over = $state(false);
	let picker: HTMLInputElement | undefined = $state();
	let calendarName = $state('');
	let link = $state('');

	const title = (source: SourceView) => source.name ?? i18n.t.sourceKinds[source.connector] ?? source.connector;
	const nameOf = (instance: string | null, sources: SourcesView) =>
		sources.sources.find((s) => s.instance === instance)?.name ?? '';

	/** The words for what a change did. */
	function said(change: SourcesChange): { tone: 'done' | 'warn'; text: string } {
		const t = i18n.t;
		const name = nameOf(change.source, change.sources);
		switch (change.outcome) {
			case 'followed':
				return { tone: 'done', text: t.followedNow(name) };
			case 'kept':
				return { tone: 'warn', text: t.keptNow(name) };
			case 'unfollowed':
				return { tone: 'done', text: t.unfollowedNow(change.detail ?? '') };
			case 'already':
				return { tone: 'warn', text: t.alreadyFollowed(change.detail ?? '') };
			case 'imported':
			case 'synced':
				return { tone: 'done', text: t.sourceOutcomes[change.outcome] };
			case 'failed': {
				const why = t.runProblems[change.detail ?? ''];
				return { tone: 'warn', text: why ? `${t.sourceOutcomes.failed} ${why}` : t.sourceOutcomes.failed };
			}
			default:
				return { tone: 'warn', text: t.sourceOutcomes[change.outcome] ?? t.actionFailed };
		}
	}

	async function run(work: NonNullable<typeof working>, action: () => Promise<SourcesChange>): Promise<SourcesChange | null> {
		working = work;
		outcome = null;
		let change: SourcesChange | null = null;
		try {
			change = await action();
			screen.state = { kind: 'ready', value: change.sources };
			outcome = said(change);
			await app.refresh();
		} catch (error) {
			if (error instanceof SessionEnded) app.state = { kind: 'ended' };
			outcome = { tone: 'warn', text: i18n.t.actionFailed };
		}
		working = null;
		return change;
	}

	function importFiles(files: FileList | null | undefined) {
		const file = files?.[0];
		if (!file || working) return;
		run({ kind: 'import', name: file.name }, () => api.importFile(file));
		if (picker) picker.value = '';
	}

	function drop(event: DragEvent) {
		event.preventDefault();
		over = false;
		importFiles(event.dataTransfer?.files);
	}

	async function follow(event: SubmitEvent) {
		event.preventDefault();
		const change = await run({ kind: 'follow' }, () => api.followCalendar(link.trim(), calendarName.trim()));
		// The link leaves the page once it is kept.
		if (change && (change.outcome === 'followed' || change.outcome === 'kept')) {
			link = '';
			calendarName = '';
		}
	}

	function ask(source: SourceView): Promise<boolean> {
		const t = i18n.t;
		return new Promise(
			(resolve) => (question = { title: t.unfollowTitle(title(source)), body: t.unfollowBody, yes: t.unfollow, no: t.keepFollowing, resolve })
		);
	}

	async function unfollow(source: SourceView) {
		if (await ask(source)) run({ kind: 'sync', instance: source.instance }, () => api.unfollow(source.instance));
	}

	/** "Importé 2 fois, dernier import il y a 3 minutes." */
	function status(source: SourceView): string {
		const t = i18n.t;
		const when = source.last ? ago(source.last.at) : null;
		if (source.followed) {
			const every = source.every_minutes ? t.followedEvery(source.every_minutes) : '';
			return `${every}, ${when ? t.lastSync(when) : t.neverSynced}.`;
		}
		return `${t.importedFiles(source.runs)}${when ? `, ${t.lastImport(when)}` : ''}.`;
	}
</script>

<svelte:head><title>{i18n.t.sourcesNav} · Ipse</title></svelte:head>

<div class="screen sources">
	<header class="page-head">
		<h1>{i18n.t.sourcesNav}</h1>
		<p class="intro">{i18n.t.sourcesLead}</p>
	</header>

	<div class="live" role="status" aria-live="polite">
		{#if working?.kind === 'import'}
			<p class="quiet">{i18n.t.importing(working.name)}</p>
		{:else if outcome}
			<Notice tone={outcome.tone}>{outcome.text}</Notice>
		{/if}
	</div>

	<div class="add">
		<section aria-labelledby="import">
			<h2 id="import">{i18n.t.importTitle}</h2>
			<p class="quiet">{i18n.t.importHint}</p>
			<div
				class="drop"
				class:over
				role="group"
				aria-labelledby="import"
				ondragover={(e) => {
					e.preventDefault();
					over = true;
				}}
				ondragleave={() => (over = false)}
				ondrop={drop}
			>
				<p>
					{i18n.t.dropHere}
					<label class="link-button" for="file">{i18n.t.chooseFile}</label>
				</p>
				<input
					id="file"
					class="visually-hidden"
					type="file"
					accept=".vcf,.vcard,.ics,.ical,.json"
					bind:this={picker}
					disabled={working !== null}
					onchange={(e) => importFiles(e.currentTarget.files)}
				/>
			</div>
			<details class="exports">
				<summary>{i18n.t.howToExport}</summary>
				{#each i18n.t.exportSteps as step (step.title)}
					<h3>{step.title}</h3>
					<ul>
						{#each step.lines as line (line)}<li>{line}</li>{/each}
					</ul>
				{/each}
			</details>
		</section>

		<section aria-labelledby="follow">
			<h2 id="follow">{i18n.t.followTitle}</h2>
			<p class="quiet" id="follow-hint">{i18n.t.followHint}</p>
			<form class="setting-form" onsubmit={follow}>
				<div class="form-field">
					<label for="calendar-name">{i18n.t.calendarName}</label>
					<input id="calendar-name" type="text" bind:value={calendarName} autocomplete="off" />
				</div>
				<div class="form-field">
					<label for="calendar-link">{i18n.t.calendarLink}</label>
					<input
						id="calendar-link"
						type="url"
						bind:value={link}
						required
						autocomplete="off"
						spellcheck="false"
						aria-describedby="follow-hint"
					/>
				</div>
				<div class="actions">
					<button type="submit" class="button primary" disabled={working !== null || !link.trim()}>
						{working?.kind === 'follow' ? i18n.t.following : i18n.t.followGo}
					</button>
				</div>
			</form>
		</section>
	</div>

	<section aria-labelledby="yours">
		<h2 id="yours">{i18n.t.yourSources}</h2>
		{#if view && !view.sources.length}
			<p class="quiet">{i18n.t.noSources}</p>
		{:else if view}
			<ul class="source-list" data-testid="sources">
				{#each view.sources as source (source.instance)}
					<li class="source">
						<div class="source-text">
							<h3>{title(source)}</h3>
							<p class="quiet">{status(source)}</p>
							{#if source.last && source.last.outcome !== 'done'}
								<Notice tone="warn">{i18n.t.runProblems[source.last.code ?? ''] ?? i18n.t.runFailed}</Notice>
							{:else if source.last}
								<p>{runCounts(source.last)}</p>
							{/if}
							{#if source.followed && source.failures > 1}
								<p class="quiet">{i18n.t.failingSince(source.failures)}</p>
							{/if}
						</div>
						{#if source.followed}
							<div class="actions">
								<button
									type="button"
									class="button small"
									disabled={working !== null}
									onclick={() => run({ kind: 'sync', instance: source.instance }, () => api.syncSource(source.instance))}
									>{working?.kind === 'sync' && working.instance === source.instance ? i18n.t.syncing : i18n.t.syncNow}</button
								>
								<button type="button" class="button small" disabled={working !== null} onclick={() => unfollow(source)}
									>{i18n.t.unfollow}</button
								>
							</div>
						{/if}
					</li>
				{/each}
			</ul>
		{:else if screen.state.kind === 'failed'}
			<p class="quiet">{i18n.t.failed}</p>
		{/if}
	</section>
</div>

<Confirm bind:question />
