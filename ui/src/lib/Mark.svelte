<script lang="ts">
	// Ipse's mark: an i whose stem loops back on itself (ipse: oneself).
	let { size = 28 }: { size?: number } = $props();
</script>

<svg class="mark" width={size} height={size} viewBox="0 0 40 40" fill="none" aria-hidden="true">
	<path d="M20 35V19.5a7.5 7.5 0 1 1 7.5 7.5" stroke="currentColor" stroke-width="4.2" stroke-linecap="round" />
	<circle cx="20" cy="7.5" r="3.4" fill="currentColor" />
</svg>
