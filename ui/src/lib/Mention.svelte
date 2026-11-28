<script lang="ts">
	// A link to another file of the memory, or a name as written.
	import type { LinkView } from './api.ts';
	import Face from './Face.svelte';
	import { hrefOf, longDay } from './format.ts';
	import { i18n } from './i18n.svelte.ts';
	let { link }: { link: LinkView } = $props();
	const href = $derived(hrefOf(link));
	// A note of the day is named after its day (issue #277).
	const name = $derived(link.kind === 'note' ? i18n.t.dayNoteOf(longDay(link.name)) : link.name);
</script>

{#if href}
	<a class="mention" {href}>
		{#if link.kind === 'person'}<Face name={link.name} size={22} />{/if}
		<span>{name}</span>
	</a>
{:else}
	<span class="mention plain">{name}</span>
{/if}
