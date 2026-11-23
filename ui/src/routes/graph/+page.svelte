<script lang="ts">
	// The graph has no screen of its own any more (issue #174): it opens in Mémoire,
	// beside the list. An old address still leads there, centred on the same file.
	import { onMount } from 'svelte';
	import { page } from '$app/state';
	import { goto } from '$app/navigation';

	onMount(() => {
		const id = page.url.searchParams.get('id');
		goto(`/memory?graph=1${id ? `&id=${encodeURIComponent(id)}` : ''}`, { replace: true });
	});
</script>
