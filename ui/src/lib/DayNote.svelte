<script lang="ts">
	// The note of the day (issue #277): what the person jots down without an AI, each
	// line with its time, mentions as links. Today's can be added to.
	import { api, SessionEnded, type DayNoteView } from './api.ts';
	import { app } from './app.svelte.ts';
	import { i18n } from './i18n.svelte.ts';
	import Mention from './Mention.svelte';
	import Notice from './Notice.svelte';
	import QuickNote from './QuickNote.svelte';

	let {
		note = $bindable(),
		editable,
		heading = 2
	}: {
		note: DayNoteView | null;
		/** Today's: a field adds to it. */
		editable: boolean;
		heading?: 2 | 3;
	} = $props();

	let outcome: { tone: 'done' | 'warn'; text: string } | null = $state(null);

	async function save(text: string) {
		try {
			note = await api.addNote(text);
			outcome = { tone: 'done', text: i18n.t.dayNoteSaved };
		} catch (error) {
			if (error instanceof SessionEnded) app.state = { kind: 'ended' };
			outcome = { tone: 'warn', text: i18n.t.noteFailed };
			throw error;
		}
	}
</script>

<section aria-labelledby="day-note" class="day-note" data-testid="day-note">
	<svelte:element this={`h${heading}`} id="day-note">{i18n.t.dayNoteTitle}</svelte:element>
	{#if editable}
		<QuickNote id="day-note-field" label={i18n.t.dayNoteLabel} placeholder={i18n.t.dayNotePlaceholder} mentions {save} />
		<p class="hint">{i18n.t.dayNoteHint}</p>
		<div class="live" role="status" aria-live="polite">
			{#if outcome}<Notice tone={outcome.tone}>{outcome.text}</Notice>{/if}
		</div>
	{/if}
	{#if note?.lines.length}
		<ul class="day-note-lines">
			{#each note.lines as line, i (i)}
				<li>
					{#each line.parts as part, j (j)}{#if part.link}<Mention link={part.link} />{:else}{part.text}{/if}{/each}
				</li>
			{/each}
		</ul>
	{:else if !editable}
		<p class="quiet">{i18n.t.dayNoteEmpty}</p>
	{/if}
</section>
