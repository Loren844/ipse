<script lang="ts">
	// People now live in the Memory section: older addresses still lead there.
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	onMount(() => goto(`/memory/people/${page.params.id}`, { replace: true }));
</script>
