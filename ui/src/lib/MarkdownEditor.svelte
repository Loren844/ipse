<script lang="ts">
	// A file of the memory as written, front matter included, in a plain text area
	// (issue #142). Saving checks that it still reads as a memory file of the same id
	// and type; a refusal says what is wrong and keeps what was typed.
	import { onMount, untrack } from 'svelte';
	import { api, Conflict, SessionEnded } from './api.ts';
	import { app } from './app.svelte.ts';
	import { i18n } from './i18n.svelte.ts';
	import Notice from './Notice.svelte';

	let {
		id,
		save,
		cancel,
		dirty = $bindable(false)
	}: { id: string; save: (version: string, text: string) => Promise<void>; cancel: () => void; dirty?: boolean } = $props();

	let read: { version: string; text: string } | null = $state(null);
	let text = $state('');
	let failed = $state(false);
	let busy = $state(false);
	let problem: string | null = $state(null);
	let area: HTMLTextAreaElement | undefined = $state();

	onMount(async () => {
		try {
			const file = await api.fileText(untrack(() => id));
			read = file;
			text = file.text;
		} catch (error) {
			if (error instanceof SessionEnded) app.state = { kind: 'ended' };
			failed = true;
		}
	});

	$effect(() => {
		dirty = !!read && text !== read.text;
	});

	/** The server's refusal, in the person's words. */
	function refusal(message: string): string {
		if (message.includes('changed since')) return i18n.t.changedMeanwhile;
		if (i18n.language === 'en') return message;
		const line = /^Line (\d+)/.exec(message)?.[1];
		if (line) return i18n.t.markdownLine(Number(line));
		const key = /`(id|type|owner|category)`/.exec(message)?.[1];
		if (key === 'category') return i18n.t.markdownCategory;
		if (key) return i18n.t.markdownFixed(key);
		if (message.includes('front matter')) return i18n.t.markdownFront;
		return i18n.t.cannotKeepFile;
	}

	async function submit(e: SubmitEvent) {
		e.preventDefault();
		if (!read) return;
		busy = true;
		problem = null;
		try {
			await save(read.version, text);
		} catch (error) {
			if (error instanceof SessionEnded) app.state = { kind: 'ended' };
			problem = error instanceof Conflict ? refusal(error.message) : i18n.t.actionFailed;
			area?.focus();
		}
		busy = false;
	}
</script>

<form class="editor markdown-editor" onsubmit={submit} novalidate>
	{#if failed}
		<Notice tone="warn">{i18n.t.actionFailed}</Notice>
	{:else}
		<div class="form-field">
			<label for="edit-markdown">{i18n.t.markdownLabel}</label>
			<textarea
				id="edit-markdown"
				bind:this={area}
				bind:value={text}
				rows="22"
				spellcheck="false"
				autocapitalize="off"
				readonly={!read}
				aria-describedby="markdown-hint{problem ? ' markdown-problem' : ''}"
			></textarea>
			<p class="hint" id="markdown-hint">{i18n.t.markdownHint}</p>
		</div>
	{/if}

	<div class="live" role="status" aria-live="polite" id="markdown-problem">
		{#if problem}<Notice tone="warn">{problem}</Notice>{/if}
	</div>

	<div class="actions">
		<button type="submit" class="button primary" disabled={busy || !read}>{i18n.t.saveFile}</button>
		<button type="button" class="button" onclick={cancel}>{i18n.t.cancelForm}</button>
	</div>
</form>
