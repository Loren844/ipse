<script lang="ts">
	// A note the first time a screen opens (issue #172): what to do there, on the thread,
	// until the person says they read it.
	import { api } from './api.ts';
	import { app } from './app.svelte.ts';
	import { i18n } from './i18n.svelte.ts';

	let { screen }: { screen: string } = $props();
	let read = $state(false);
	const seen = $derived(app.state.kind === 'ready' ? app.state.info.seen.includes(screen) : true);

	async function done() {
		read = true;
		try {
			const answer = await api.sawScreen(screen);
			if (app.state.kind === 'ready') app.state = { kind: 'ready', info: { ...app.state.info, seen: answer.seen } };
		} catch {
			// Shown again at the next visit: nothing is lost.
		}
	}
</script>

{#if !seen && !read}
	<aside class="first-visit" data-testid="first-visit">
		<p>{i18n.t.firstVisit[screen]}</p>
		<button type="button" class="button small" onclick={done}>{i18n.t.screenNoteRead}</button>
	</aside>
{/if}
