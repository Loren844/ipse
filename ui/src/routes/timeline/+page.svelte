<script lang="ts">
	// The timeline is the agenda's Years view (issue #211): an old address leads there,
	// at the same year and month.
	import { onMount } from 'svelte';
	import { page } from '$app/state';
	import { goto } from '$app/navigation';

	onMount(() => {
		const query = new URLSearchParams({ view: 'years' });
		for (const key of ['year', 'month']) {
			const value = page.url.searchParams.get(key);
			if (value) query.set(key, value);
		}
		goto(`/agenda?${query}`, { replace: true });
	});
</script>
