<script lang="ts">
	let { title, children }: { title: string; children?: import('svelte').Snippet } = $props();
</script>

<section class="empty">
	<h1>{title}</h1>
	{#if children}<div class="hint">{@render children()}</div>{/if}
</section>
