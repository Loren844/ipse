<script lang="ts">
	// The outcome of an action, announced to screen readers: done (green, a check) or a
	// warning (ochre, an exclamation mark). Never the thread's red, never colour alone.
	let { tone = 'done', children }: { tone?: 'done' | 'warn'; children: import('svelte').Snippet } = $props();
</script>

<p class="notice" class:warn={tone === 'warn'}>
	{#if tone === 'warn'}
		<svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"
			><path d="M12 3.5 21 19.5H3z" stroke-linejoin="round" /><path d="M12 10v4.5M12 17.2v.1" /></svg
		>
	{:else}
		<svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"
			><path d="M5 12.5l4.5 4.5L19 7.5" /></svg
		>
	{/if}
	<span>{@render children()}</span>
</p>
