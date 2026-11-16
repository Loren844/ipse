<script lang="ts">
	// A list of short values the person edits one by one: other names, phones, emails.
	import { tick } from 'svelte';
	import { i18n } from './i18n.svelte.ts';

	let {
		values = $bindable(),
		label,
		add,
		id,
		type = 'text',
		autocomplete = 'off'
	}: {
		values: string[];
		label: string;
		add: string;
		id: string;
		type?: 'text' | 'email' | 'tel';
		autocomplete?: AutoFill;
	} = $props();
	let list: HTMLElement | undefined = $state();

	async function more() {
		values = [...values, ''];
		await tick();
		list?.querySelector<HTMLInputElement>('li:last-child input')?.focus();
	}

	async function remove(at: number) {
		values = values.filter((_, i) => i !== at);
		await tick();
		const inputs = list?.querySelectorAll<HTMLInputElement>('input');
		(inputs?.[Math.min(at, (inputs?.length ?? 1) - 1)] ?? list?.parentElement?.querySelector<HTMLButtonElement>('.add'))?.focus();
	}
</script>

<fieldset class="list-input">
	<legend>{label}</legend>
	<ul bind:this={list}>
		{#each values as _, i (i)}
			<li>
				<input
					id="{id}-{i}"
					{type}
					{autocomplete}
					spellcheck="false"
					aria-label="{label} {i + 1}"
					bind:value={values[i]}
				/>
				<button type="button" class="remove" aria-label={i18n.t.removeItem(values[i] || `${label} ${i + 1}`)} onclick={() => remove(i)}
					><svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true"><path d="M6 6l12 12M18 6 6 18" /></svg></button
				>
			</li>
		{/each}
	</ul>
	<button type="button" class="link-button add" onclick={more}>+ {add}</button>
</fieldset>
