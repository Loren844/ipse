<script lang="ts">
	// A link to another file of the memory, or a name as written.
	import type { LinkView } from './api.ts';
	import Face from './Face.svelte';
	import { hrefOf } from './format.ts';
	let { link }: { link: LinkView } = $props();
	const href = $derived(hrefOf(link));
</script>

{#if href}
	<a class="mention" {href}>
		{#if link.kind === 'person'}<Face name={link.name} size={22} />{/if}
		<span>{link.name}</span>
	</a>
{:else}
	<span class="mention plain">{link.name}</span>
{/if}
