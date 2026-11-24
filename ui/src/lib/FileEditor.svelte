<script lang="ts">
	// Modifier opens this (issue #142): the file as a form (Fiche) or as written
	// (Markdown), on the tab chosen last. Switching saves nothing; with changes not
	// saved yet, it asks first, in the page's own dialog (`question`).
	import { tick } from 'svelte';
	import type { Draft, EditView } from './api.ts';
	import type { Question } from './Confirm.svelte';
	import Editor from './Editor.svelte';
	import { i18n } from './i18n.svelte.ts';
	import MarkdownEditor from './MarkdownEditor.svelte';

	let {
		form,
		save,
		saveText,
		cancel,
		question = $bindable(null)
	}: {
		form: EditView;
		save: (draft: Draft) => Promise<void>;
		saveText: (version: string, text: string) => Promise<void>;
		cancel: () => void;
		question?: Question | null;
	} = $props();

	type Tab = 'form' | 'markdown';
	const KEY = 'ipse-editor-tab';
	let tab: Tab = $state(
		(() => {
			try {
				return localStorage.getItem(KEY) === 'markdown' ? 'markdown' : 'form';
			} catch {
				return 'form';
			}
		})()
	);
	let dirty = $state(false);

	async function show(next: Tab, focus = false) {
		if (next === tab) return;
		if (dirty) {
			const drop = await new Promise<boolean>((resolve) => {
				question = { title: i18n.t.switchTitle, body: i18n.t.switchBody, yes: i18n.t.switchYes, no: i18n.t.switchNo, resolve };
			});
			if (!drop) return;
		}
		dirty = false;
		tab = next;
		try {
			localStorage.setItem(KEY, next);
		} catch {
			// The choice then lasts until the page is closed.
		}
		if (focus) {
			await tick();
			document.getElementById(`tab-${next}`)?.focus();
		}
	}

	/** Arrow keys move between the tabs, as in any tab list. */
	function keys(e: KeyboardEvent) {
		if (e.key === 'ArrowRight' || e.key === 'ArrowLeft' || e.key === 'Home' || e.key === 'End') {
			e.preventDefault();
			const next: Tab = e.key === 'Home' ? 'form' : e.key === 'End' ? 'markdown' : tab === 'form' ? 'markdown' : 'form';
			show(next, true);
		}
	}
</script>

<div class="segmented editor-tabs" role="tablist" aria-label={i18n.t.editAs}>
	<button
		type="button"
		role="tab"
		id="tab-form"
		aria-selected={tab === 'form'}
		aria-controls="panel-form"
		tabindex={tab === 'form' ? 0 : -1}
		onclick={() => show('form')}
		onkeydown={keys}>{i18n.t.editAsForm}</button
	>
	<button
		type="button"
		role="tab"
		id="tab-markdown"
		aria-selected={tab === 'markdown'}
		aria-controls="panel-markdown"
		tabindex={tab === 'markdown' ? 0 : -1}
		onclick={() => show('markdown')}
		onkeydown={keys}>{i18n.t.editAsMarkdown}</button
	>
</div>

{#if tab === 'form'}
	<div role="tabpanel" id="panel-form" aria-labelledby="tab-form">
		<Editor {form} {save} {cancel} bind:dirty />
	</div>
{:else}
	<div role="tabpanel" id="panel-markdown" aria-labelledby="tab-markdown">
		<MarkdownEditor id={form.id} save={saveText} {cancel} bind:dirty />
	</div>
{/if}
