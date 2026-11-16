<script lang="ts">
	// The history's changes: who, what, which files, and undo as a new change.
	import { api, Conflict, SessionEnded, type ChangeLine } from './api.ts';
	import { app } from './app.svelte.ts';
	import Confirm, { type Question } from './Confirm.svelte';
	import { ago, capitalize, sources } from './format.ts';
	import { i18n } from './i18n.svelte.ts';
	import Mention from './Mention.svelte';
	import Notice from './Notice.svelte';

	let changes: ChangeLine[] = $state([]);
	let more = $state(false);
	let loaded = $state(false);
	let failed = $state(false);
	let busy = $state(false);
	let outcome: { tone: 'done' | 'warn'; text: string } | null = $state(null);
	let question: Question | null = $state(null);

	async function load(offset = 0) {
		try {
			const page = await api.history(offset);
			changes = offset ? [...changes, ...page.changes] : page.changes;
			more = page.more;
			loaded = true;
		} catch (error) {
			if (error instanceof SessionEnded) app.state = { kind: 'ended' };
			failed = true;
		}
	}
	load();

	function who(change: ChangeLine): string {
		const { kind, name, author } = change.by;
		if (kind === 'person') return i18n.t.byPerson;
		if (kind === 'ipse') return i18n.t.changedByIpse;
		if (kind === 'source') return capitalize(sources([name ?? '']));
		if (kind === 'proposal') {
			if (author === 'ai') return i18n.t.byProposalAi(name ?? '');
			if (author === 'source') return i18n.t.byProposalSource(sources([name ?? '']));
			return i18n.t.byProposalIpse;
		}
		return i18n.t.byUnknown;
	}

	function what(change: ChangeLine): string {
		if (change.summary.startsWith('Sync ')) return i18n.t.syncSummary;
		return i18n.t.summaries[change.summary] ?? change.summary;
	}

	async function undo(change: ChangeLine) {
		const yes = await new Promise<boolean>(
			(resolve) =>
				(question = { title: i18n.t.undoChangeTitle, body: i18n.t.undoChangeBody, yes: i18n.t.undoChange, no: i18n.t.cancelForm, resolve })
		);
		if (!yes) return;
		busy = true;
		try {
			const result = await api.undoChange(change.id);
			outcome = result.changed ? { tone: 'done', text: i18n.t.changeUndone } : { tone: 'warn', text: i18n.t.nothingToUndo };
			await load();
			await app.refresh();
		} catch (error) {
			if (error instanceof SessionEnded) app.state = { kind: 'ended' };
			outcome = { tone: 'warn', text: error instanceof Conflict ? i18n.t.cannotUndoChange : i18n.t.actionFailed };
		}
		busy = false;
	}
</script>

<div class="live" role="status" aria-live="polite">
	{#if outcome}<Notice tone={outcome.tone}>{outcome.text}</Notice>{/if}
</div>

{#if failed}
	<p class="quiet">{i18n.t.failed}</p>
{:else if loaded && !changes.length}
	<p class="quiet">{i18n.t.historyEmpty}</p>
{:else}
	<ol class="thread history" data-testid="changes">
		{#each changes as change (change.id)}
			<li class="knot">
				<span class="when">{ago(change.at)}</span>
				<span class="title-line"><strong>{who(change)}</strong> <span class="quiet">{what(change)}</span></span>
				{#if change.files.length}
					<span class="mentions">
						{#each change.files as file, i (i)}<Mention link={file} />{/each}
						{#if change.more_files}<span class="quiet more-files">{i18n.t.moreFiles(change.more_files)}</span>{/if}
					</span>
				{/if}
				{#if change.by.kind !== 'ipse' && change.files.length}
					<button type="button" class="link-button" disabled={busy} onclick={() => undo(change)}>{i18n.t.undoChange}</button>
				{/if}
			</li>
		{/each}
	</ol>
	{#if more}
		<button type="button" class="more" onclick={() => load(changes.length)}>{i18n.t.olderChanges}</button>
	{/if}
{/if}
<Confirm bind:question />
