<script lang="ts">
	// One question from the inbox: who is someone saved under a first name or a
	// nickname. What the memory knows helps the person remember; the answer completes
	// the file, the old name stays as another name.
	import { api, Conflict, SessionEnded, type QuestionsView, type QuestionView } from './api.ts';
	import { app } from './app.svelte.ts';
	import Face from './Face.svelte';
	import { longDay, phone } from './format.ts';
	import { i18n } from './i18n.svelte.ts';

	let {
		question,
		today,
		done
	}: {
		question: QuestionView;
		today?: string;
		/** The questions left, and what to tell the person. */
		done: (left: QuestionsView | null, tone: 'done' | 'warn', text: string) => void;
	} = $props();

	let name = $state('');
	let word = $state('');
	let busy = $state(false);

	const key = $derived(question.id);

	async function decide(action: () => Promise<QuestionsView>, success: string) {
		busy = true;
		try {
			done(await action(), 'done', success);
		} catch (error) {
			if (error instanceof SessionEnded) app.state = { kind: 'ended' };
			done(null, 'warn', error instanceof Conflict ? i18n.t.questionGone : i18n.t.actionFailed);
		}
		busy = false;
	}

	function save(e: SubmitEvent) {
		e.preventDefault();
		const full = name.trim();
		if (!full) return;
		decide(() => api.answer(question.id, full, word), i18n.t.answered(full));
	}
</script>

<li class="question" data-testid="question" aria-labelledby="q-{key}">
	<div class="asked">
		<Face name={question.name} size={44} />
		<h3 id="q-{key}">{i18n.t.questionTitle(question.name)}</h3>
	</div>

	{#if question.phones.length || question.emails.length || question.events.length}
		<ul class="clues" aria-label={i18n.t.questionClues(question.name)}>
			{#each question.phones as number (number)}<li><span class="clue">{i18n.t.phone}</span>{phone(number)}</li>{/each}
			{#each question.emails as email (email)}<li><span class="clue">{i18n.t.email}</span>{email}</li>{/each}
			{#each question.events as event, i (i)}
				<li><span class="clue">{i18n.t.seenWith}</span>{i18n.t.seenAt(event.title, longDay(event.day, today))}</li>
			{/each}
		</ul>
	{/if}

	<form class="answer" onsubmit={save}>
		<div class="field">
			<label for="name-{key}">{i18n.t.fullName}</label>
			<input id="name-{key}" type="text" bind:value={name} maxlength="200" autocomplete="off" required />
		</div>
		<div class="field">
			<label for="word-{key}">{i18n.t.howKnown}</label>
			<input id="word-{key}" type="text" bind:value={word} maxlength="60" autocomplete="off" aria-describedby="word-hint-{key}" />
			<p class="hint" id="word-hint-{key}">{i18n.t.howKnownHint}</p>
		</div>
		<div class="actions">
			<button type="submit" class="button primary" disabled={busy || !name.trim()}>{i18n.t.saveAnswer}</button>
			<button
				type="button"
				class="button"
				disabled={busy}
				aria-describedby="skip-hint-{key}"
				onclick={() => decide(() => api.skipQuestion(question.id), i18n.t.questionSkipped)}>{i18n.t.dontKnow}</button
			>
			<span class="hint" id="skip-hint-{key}">{i18n.t.dontKnowHint}</span>
			<a class="open" href="/people/{question.person}">{i18n.t.openPerson}</a>
		</div>
	</form>
</li>
