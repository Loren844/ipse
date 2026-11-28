<script lang="ts">
	import { page } from '$app/state';
	import { tick } from 'svelte';
	import { api, Conflict, SessionEnded, type DecisionView, type ProposalView } from '#lib/api.ts';
	import { app } from '#lib/app.svelte.ts';
	import Diff from '#lib/Diff.svelte';
	import { ago, authorName } from '#lib/format.ts';
	import { i18n } from '#lib/i18n.svelte.ts';
	import Notice from '#lib/Notice.svelte';
	import { title } from '#lib/proposal.ts';
	import { Loader } from '#lib/remote.svelte.ts';

	const proposal = new Loader<ProposalView>();
	let outcome: { tone: 'done' | 'warn'; text: string } | null = $state(null);
	let refusing = $state(false);
	let comment = $state('');
	let busy = $state(false);
	let next: string | null = $state(null);
	let commentField: HTMLTextAreaElement | undefined = $state();
	let status: HTMLElement | undefined = $state();

	$effect(() => {
		const id = page.params.id;
		if (!id) return;
		outcome = null;
		refusing = false;
		comment = '';
		next = null;
		proposal.load(() => api.proposal(id));
	});

	const view = $derived(proposal.state.kind === 'ready' ? proposal.state.value : null);
	const line = $derived(view?.line);
	const by = $derived.by(() => {
		if (!line) return '';
		if (line.by.kind === 'source') return i18n.t.changedBy(authorName(line.by));
		if (line.by.kind === 'ipse') return i18n.t.byIpse;
		return i18n.t.proposedBy(authorName(line.by));
	});

	/** Runs a decision, then shows the proposal as it now stands and the next one waiting. */
	async function decide(action: () => Promise<DecisionView>, done: (result: DecisionView) => typeof outcome, conflict: string) {
		if (!line) return;
		const id = line.id;
		busy = true;
		try {
			outcome = done(await action());
			refusing = false;
		} catch (error) {
			if (error instanceof SessionEnded) app.state = { kind: 'ended' };
			outcome = { tone: 'warn', text: error instanceof Conflict ? conflict : i18n.t.actionFailed };
		}
		busy = false;
		const [, inbox] = await Promise.all([proposal.load(() => api.proposal(id), true), api.proposals().catch(() => null), app.refresh()]);
		next = inbox?.waiting.find((p) => p.id !== id)?.id ?? null;
		await tick();
		status?.focus();
	}

	const merge = $derived(line?.merge ?? false);
	/** For a merge, "these are two people", or "keep them apart" for events. */
	const apart = $derived(view?.line.files.some((f) => f.kind === 'person') ? i18n.t.twoPeople : i18n.t.keepApart);
	const accept = () =>
		decide(
			() => api.accept(line!.id),
			(r) =>
				r.decided[0]?.status === 'stale'
					? { tone: 'warn', text: i18n.t.becameStale(1) }
					: { tone: 'done', text: merge ? i18n.t.mergedDone : i18n.t.accepted(1) },
			i18n.t.alreadyDecided
		);
	const refuse = () =>
		decide(
			() => api.refuse(line!.id, comment.trim()),
			() => ({ tone: 'done', text: merge ? i18n.t.keptApartDone : i18n.t.refusedDone }),
			i18n.t.alreadyDecided
		);
	const undo = () => decide(() => api.undo(line!.id), () => ({ tone: 'done', text: i18n.t.undoneDone }), i18n.t.cannotUndo);

	async function startRefusing() {
		refusing = true;
		await tick();
		commentField?.focus();
	}
</script>

<svelte:head><title>{line ? title(line) : i18n.t.inbox} · Ipse</title></svelte:head>

{#if view && line}
	<article class="screen">
		<header class="page-head">
			<p class="quiet-link" style:margin="0"><a href="/review">← {i18n.t.backToInbox}</a></p>
			<!-- What it changes is the title; why, in its author's words, comes under it. -->
			<h1 class="proposal-title">{title(line)}</h1>
			<p class="why"><span class="visually-hidden">{`${i18n.t.why} `}</span>{line.reason}</p>
			<p class="date">
				<span>{by}, {ago(line.created)}</span>
				{#if line.on_its_own}<span class="badge" data-testid="on-its-own">{i18n.t.notedOnItsOwn}</span>{/if}
				{#if line.status !== 'waiting'}<span class="badge" class:warn={line.status === 'stale'}>{i18n.t.status[line.status]}</span>{/if}
			</p>
		</header>

		<div class="live" role="status" aria-live="polite" tabindex="-1" bind:this={status}>
			{#if outcome}
				<Notice tone={outcome.tone}>
					{outcome.text}
					{#if next}<a href="/review/{next}">{i18n.t.nextOne}</a>{/if}
				</Notice>
			{/if}
		</div>

		{#if view.stale && (line.status === 'waiting' || line.status === 'stale')}
			<Notice tone="warn"><strong>{i18n.t.staleTitle}.</strong> {i18n.t.staleHint}</Notice>
		{/if}

		{#each view.line.files as file, i (i)}<Diff {file} />{/each}

		{#if line.status === 'refused' && view.comment}<p class="quiet">{i18n.t.youSaid(view.comment)}</p>{/if}

		{#if line.status === 'waiting' || line.status === 'stale'}
			{#if refusing}
				<form
					class="refuse"
					onsubmit={(e) => {
						e.preventDefault();
						refuse();
					}}
				>
					<label for="comment">{i18n.t.refuseWhy}</label>
					<textarea id="comment" bind:value={comment} bind:this={commentField} maxlength="2000" aria-describedby="comment-hint"></textarea>
					<p class="hint" id="comment-hint">{i18n.t.refuseWhyHint}</p>
					<div class="actions">
						<button type="submit" class="button primary" disabled={busy}>{i18n.t.confirmRefuse}</button>
						<button type="button" class="button" onclick={() => (refusing = false)}>{i18n.t.cancel}</button>
					</div>
				</form>
			{:else}
				<div class="actions">
					{#if line.status === 'waiting' && !view.stale}
						<button type="button" class="button primary" disabled={busy} onclick={accept}>{merge ? i18n.t.mergeThem : i18n.t.accept}</button>
					{/if}
					{#if merge}
						<!-- Saying they are different needs no reason: it is remembered. -->
						<button type="button" class="button" disabled={busy} onclick={refuse}>{apart}</button>
					{:else}
						<button type="button" class="button" disabled={busy} onclick={startRefusing}>{i18n.t.refuse}</button>
					{/if}
				</div>
			{/if}
		{:else if line.status === 'accepted'}
			<div class="actions">
				<button type="button" class="button" disabled={busy} onclick={undo} aria-describedby="undo-hint">{i18n.t.undo}</button>
				<span class="quiet" id="undo-hint">{i18n.t.undoHint}</span>
			</div>
		{/if}
	</article>
{:else if proposal.state.kind === 'missing'}
	<div class="screen"><h1>{i18n.t.noProposal}</h1><p><a href="/review">{i18n.t.backToInbox}</a></p></div>
{:else if proposal.state.kind === 'failed'}
	<p class="quiet">{i18n.t.failed}</p>
{/if}
