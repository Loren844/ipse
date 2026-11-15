<script lang="ts">
	import { app } from '#lib/app.svelte.ts';
	import { i18n } from '#lib/i18n.svelte.ts';
	import Empty from '#lib/Empty.svelte';

	const waiting = $derived(app.state.kind === 'ready' ? app.state.info.waiting : 0);
</script>

<Empty title={i18n.t.inbox}>
	{#if waiting > 0}
		<p class="lead">{i18n.t.inboxWaiting(waiting)}</p>
	{:else}
		<p class="lead">{i18n.t.inboxEmpty}</p>
		<p>{i18n.t.inboxEmptyHint}</p>
	{/if}
</Empty>
