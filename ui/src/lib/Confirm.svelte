<script lang="ts" module>
	export type Question = { title: string; body: string; yes: string; no: string; resolve: (yes: boolean) => void };
</script>

<script lang="ts">
	// A question before an action that opens or closes something: a modal dialog, focus
	// on the safe answer, Escape for no.
	import { tick } from 'svelte';
	let { question = $bindable() }: { question: Question | null } = $props();
	let dialog: HTMLDialogElement | undefined = $state();
	let safe: HTMLButtonElement | undefined = $state();

	$effect(() => {
		if (question && dialog && !dialog.open) {
			dialog.showModal();
			tick().then(() => safe?.focus());
		}
	});

	function answer(yes: boolean) {
		const asked = question;
		question = null;
		dialog?.close();
		asked?.resolve(yes);
	}
</script>

<dialog
	class="confirm"
	bind:this={dialog}
	aria-labelledby="confirm-title"
	aria-describedby="confirm-body"
	oncancel={(e) => {
		e.preventDefault();
		answer(false);
	}}
>
	{#if question}
		<h2 id="confirm-title">{question.title}</h2>
		<p id="confirm-body">{question.body}</p>
		<div class="actions">
			<button type="button" class="button" bind:this={safe} onclick={() => answer(false)}>{question.no}</button>
			<button type="button" class="button primary" onclick={() => answer(true)}>{question.yes}</button>
		</div>
	{/if}
</dialog>
