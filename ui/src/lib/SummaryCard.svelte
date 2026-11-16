<script lang="ts">
	// One summary: its period in words, its text, what it was written from, and the
	// person's correction in place.
	import { tick } from 'svelte';
	import { api, Conflict, SessionEnded, type SummaryView } from './api.ts';
	import { app } from './app.svelte.ts';
	import { capitalize, longDay, periodName } from './format.ts';
	import { i18n } from './i18n.svelte.ts';
	import Mention from './Mention.svelte';
	import Notice from './Notice.svelte';

	let { summary = $bindable(), today, level = 2 }: { summary: SummaryView; today: string; level?: 2 | 3 } = $props();

	let editing = $state(false);
	let draft = $state('');
	let busy = $state(false);
	let outcome: { tone: 'done' | 'warn'; text: string } | null = $state(null);
	let field: HTMLTextAreaElement | undefined = $state();
	let button: HTMLButtonElement | undefined = $state();

	const name = $derived(summary.period === 'day' ? longDay(summary.start, today) : capitalize(periodName(summary.label)));
	const paragraphs = $derived(summary.text.split(/\n{2,}/).filter((p) => p.trim()));
	const partName = (part: { period: string; start: string; label: string }) =>
		part.period === 'day' ? longDay(part.start, today) : capitalize(periodName(part.label));

	async function edit() {
		draft = summary.text;
		editing = true;
		outcome = null;
		await tick();
		field?.focus();
	}

	async function close() {
		editing = false;
		await tick();
		button?.focus();
	}

	async function save(e: SubmitEvent) {
		e.preventDefault();
		busy = true;
		try {
			summary = await api.correctSummary(summary.period, summary.start, draft);
			outcome = { tone: 'done', text: i18n.t.corrected };
			await close();
		} catch (error) {
			if (error instanceof SessionEnded) app.state = { kind: 'ended' };
			outcome = { tone: 'warn', text: error instanceof Conflict ? i18n.t.refusedChange : i18n.t.actionFailed };
		}
		busy = false;
	}
</script>

<article class="summary" id="{summary.period}-{summary.start}" aria-labelledby="title-{summary.period}-{summary.start}" data-testid="summary">
	<svelte:element this={`h${level}`} id="title-{summary.period}-{summary.start}" class="summary-title">{name}</svelte:element>

	{#if editing}
		<form class="refuse" onsubmit={save}>
			<label for="text-{summary.period}-{summary.start}" class="visually-hidden">{i18n.t.correctLabel(name)}</label>
			<textarea id="text-{summary.period}-{summary.start}" bind:value={draft} bind:this={field} maxlength="8000" rows="6"></textarea>
			<div class="actions">
				<button type="submit" class="button primary" disabled={busy || !draft.trim()}>{i18n.t.saveCorrection}</button>
				<button type="button" class="button" onclick={close}>{i18n.t.cancelForm}</button>
			</div>
		</form>
	{:else}
		<div class="summary-text">
			{#each paragraphs as paragraph, i (i)}<p>{paragraph}</p>{/each}
		</div>
	{/if}

	<div class="live" role="status" aria-live="polite">
		{#if outcome}<Notice tone={outcome.tone}>{outcome.text}</Notice>{/if}
	</div>

	{#if summary.events.length}
		<p class="mentions" aria-label={i18n.t.eventsOfDay}>
			{#each summary.events as event, i (i)}<Mention link={event} />{/each}
		</p>
	{:else if summary.parts.length}
		<p class="mentions summary-parts">
			<span class="quiet">{i18n.t.builtFrom}</span>
			{#each summary.parts as part (part.start)}
				<a class="mention plain" href="/summaries?period={part.period}&before={part.start}#{part.period}-{part.start}">{partName(part)}</a>
			{/each}
		</p>
	{/if}

	{#if !editing}
		<button type="button" class="link-button" bind:this={button} onclick={edit} aria-label="{i18n.t.correct} — {name}">{i18n.t.correct}</button>
	{/if}
</article>
