<script lang="ts">
	// Once, after the memory is created (issue #262): whether its disk is encrypted, on
	// the home screen until the person says they read it.
	import { onMount } from 'svelte';
	import { api } from './api.ts';
	import { app } from './app.svelte.ts';
	import DiskState from './DiskState.svelte';
	import { i18n } from './i18n.svelte.ts';
	import type { DiskView } from './types/DiskView.ts';

	let disk: DiskView | null = $state(null);
	let read = $state(false);
	const seen = $derived(app.state.kind === 'ready' ? app.state.info.seen.includes('disk') : true);

	onMount(async () => {
		if (seen) return;
		try {
			disk = await api.disk();
		} catch {
			// Shown at the next visit.
		}
	});

	async function done() {
		read = true;
		try {
			const answer = await api.sawScreen('disk');
			if (app.state.kind === 'ready') app.state = { kind: 'ready', info: { ...app.state.info, seen: answer.seen } };
		} catch {
			// Shown again at the next visit: nothing is lost.
		}
	}
</script>

{#if disk && !seen && !read}
	<aside class="first-visit disk-note" data-testid="disk-note" data-state={disk.state} aria-labelledby="disk-note-title">
		<div>
			<h2 id="disk-note-title">{i18n.t.diskHeading(i18n.t.diskState[disk.state])}</h2>
			<DiskState {disk} />
			<p class="hint">{i18n.t.diskWhy}</p>
		</div>
		<button type="button" class="button small" onclick={done}>{i18n.t.diskNoteDone}</button>
	</aside>
{/if}
