<script lang="ts">
	import { tick } from 'svelte';
	import {
		api,
		Conflict,
		SessionEnded,
		type DecisionView,
		type InboxView,
		type ProposalLine,
		type QuestionsView
	} from '#lib/api.ts';
	import { app } from '#lib/app.svelte.ts';
	import Empty from '#lib/Empty.svelte';
	import { ago, authorName, dayOf, periodName } from '#lib/format.ts';
	import { i18n } from '#lib/i18n.svelte.ts';
	import Notice from '#lib/Notice.svelte';
	import Question from '#lib/Question.svelte';
	import { Loader } from '#lib/remote.svelte.ts';

	const inbox = new Loader<InboxView>();
	inbox.load(() => api.proposals());
	const asked = new Loader<QuestionsView>();
	asked.load(() => api.questions());
	const today = dayOf(new Date().toISOString());

	let selected: string[] = $state([]);
	let busy = $state(false);
	let outcome: { tone: 'done' | 'warn'; text: string } | null = $state(null);
	let status: HTMLDivElement | undefined = $state();

	const ready = $derived(inbox.state.kind === 'ready' ? inbox.state.value : null);
	/** Questions are a bonus: the inbox shows its proposals even if they fail to load. */
	const questions = $derived(asked.state.kind === 'ready' ? asked.state.value.questions : []);
	/** Those that can be accepted: a stale one can only be refused, on its own page. */
	const acceptable = $derived(ready ? ready.waiting.filter((p) => p.status === 'waiting').map((p) => p.id) : []);
	const allSelected = $derived(acceptable.length > 0 && acceptable.every((id) => selected.includes(id)));

	function byline(line: ProposalLine): string {
		if (line.by.kind === 'source') return i18n.t.changedBy(authorName(line.by));
		if (line.by.kind === 'ipse') return i18n.t.byIpse;
		return i18n.t.proposedBy(authorName(line.by));
	}

	function toggleAll() {
		selected = allSelected ? [] : [...acceptable];
	}

	async function acceptSelected() {
		const ids = selected.filter((id) => acceptable.includes(id));
		if (!ids.length) return;
		busy = true;
		try {
			const result: DecisionView = await api.acceptAll(ids);
			const accepted = result.decided.filter((d) => d.status === 'accepted').length;
			const stale = result.decided.length - accepted;
			outcome = stale
				? { tone: 'warn', text: `${accepted ? i18n.t.accepted(accepted) + ' ' : ''}${i18n.t.becameStale(stale)}` }
				: { tone: 'done', text: i18n.t.accepted(accepted) };
		} catch (error) {
			if (error instanceof SessionEnded) app.state = { kind: 'ended' };
			outcome = { tone: 'warn', text: error instanceof Conflict ? i18n.t.alreadyDecided : i18n.t.actionFailed };
		}
		selected = [];
		busy = false;
		await Promise.all([inbox.load(() => api.proposals(), true), app.refresh()]);
	}

	async function answered(left: QuestionsView | null, tone: 'done' | 'warn', text: string) {
		outcome = { tone, text };
		await Promise.all([asked.load(async () => left ?? (await api.questions()), true), app.refresh()]);
		// The card answered is gone: the focus goes to what was said about it.
		await tick();
		status?.focus();
	}
</script>

<svelte:head><title>{i18n.t.inbox} · Ipse</title></svelte:head>

{#if ready && (ready.waiting.length || ready.decided.length || questions.length || outcome)}
	<div class="screen">
		<header class="page-head">
			<h1>{i18n.t.inbox}</h1>
			{#if ready.waiting.length}
				<p class="lead">{i18n.t.inboxWaiting(ready.waiting.length)}</p>
				<p class="quiet">{i18n.t.inboxHint}</p>
			{:else if !questions.length}
				<p class="lead">{i18n.t.inboxEmpty}</p>
			{/if}
		</header>

		<div class="live" role="status" aria-live="polite" tabindex="-1" bind:this={status}>
			{#if outcome}<Notice tone={outcome.tone}>{outcome.text}</Notice>{/if}
		</div>

		{#if ready.waiting.length}
			<section aria-labelledby="waiting">
				<h2 id="waiting" class="visually-hidden">{i18n.t.status.waiting}</h2>
				{#if acceptable.length}
					<div class="toolbar">
						<label><input type="checkbox" checked={allSelected} onchange={toggleAll} />{i18n.t.selectAll}</label>
						<button type="button" class="button primary" disabled={busy || selected.length === 0} onclick={acceptSelected}
							>{i18n.t.acceptSelection(Math.max(selected.length, 1))}</button
						>
					</div>
				{/if}
				<ul class="inbox" style:margin-top="1rem">
					{#each ready.waiting as line (line.id)}
						<li class="proposal" data-testid="proposal">
							{#if line.status === 'waiting'}
								<input type="checkbox" bind:group={selected} value={line.id} aria-label={i18n.t.select(line.reason)} />
							{:else}
								<span class="spacer"></span>
							{/if}
							<div class="body">
								<a class="reason" href="/review/{line.id}">{line.reason}</a>
								<p class="meta">
									<span>{byline(line)}</span>
									<span>{ago(line.created)}</span>
									<span>{i18n.t.changes(line.changes)}</span>
									{#if line.status === 'stale'}<span class="badge warn">{i18n.t.status.stale}</span>{/if}
								</p>
								<ul class="touches">
									{#each line.touches as name, i (i)}<li class="mention plain">{periodName(name)}</li>{/each}
								</ul>
							</div>
						</li>
					{/each}
				</ul>
			</section>
		{/if}

		{#if questions.length}
			<section aria-labelledby="questions" class="questions">
				<h2 id="questions">{i18n.t.questions}</h2>
				<p class="quiet">{i18n.t.questionsLead}</p>
				<ul class="inbox">
					{#each questions as question (question.id)}
						<Question {question} {today} done={answered} />
					{/each}
				</ul>
			</section>
		{/if}

		{#if ready.decided.length}
			<section aria-labelledby="decided">
				<h2 id="decided">{i18n.t.decidedRecently}</h2>
				<ul class="inbox">
					{#each ready.decided as line (line.id)}
						<li class="proposal decided" data-testid="decided">
							<div class="body">
								<a class="reason" href="/review/{line.id}">{line.reason}</a>
								<p class="meta">
									<span>{byline(line)}</span>
									<span>{i18n.t.decidedOn(i18n.t.status[line.status], ago(line.decided ?? line.created))}</span>
								</p>
							</div>
						</li>
					{/each}
				</ul>
			</section>
		{/if}
	</div>
{:else if ready}
	<Empty title={i18n.t.inbox}>
		<p class="lead">{i18n.t.inboxEmpty}</p>
		<p>{i18n.t.inboxEmptyHint}</p>
	</Empty>
{:else if inbox.state.kind === 'failed'}
	<p class="quiet">{i18n.t.failed}</p>
{/if}
