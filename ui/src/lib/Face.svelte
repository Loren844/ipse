<script lang="ts">
	import { initials } from './format.ts';
	let { name, size = 24 }: { name: string; size?: number } = $props();
</script>

<span class="face" style:width="{size}px" style:height="{size}px" style:font-size="{Math.round(size * 0.4)}px" aria-hidden="true"
	>{initials(name)}</span
>
