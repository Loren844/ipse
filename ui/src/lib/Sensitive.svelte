<script lang="ts">
	import { i18n } from './i18n.svelte.ts';
	let { category }: { category: string } = $props();
</script>

<p class="sensitive" role="note">
	<svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true"
		><rect x="5" y="10.5" width="14" height="10" rx="2.5" /><path d="M8.5 10.5V7.5a3.5 3.5 0 0 1 7 0v3" /></svg
	>
	<span>{i18n.t.sensitive(i18n.category(category))}</span>
</p>
