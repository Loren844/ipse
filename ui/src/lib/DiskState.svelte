<script lang="ts">
	// Whether the disk holding the memory is encrypted (issue #262, decision 0009): on;
	// off, with the steps for this system; or unknown. Ipse only reads it.
	import type { DiskView } from './types/DiskView.ts';
	import { i18n } from './i18n.svelte.ts';

	let { disk }: { disk: DiskView } = $props();
</script>

{#if disk.state === 'on'}
	<p>{i18n.t.diskOn[disk.system] ?? i18n.t.diskOn.other}</p>
{:else if disk.state === 'off'}
	<p><strong>{i18n.t.diskOff}</strong></p>
	<p>{i18n.t.diskSteps[disk.system] ?? i18n.t.diskSteps.other}</p>
{:else}
	<p>{i18n.t.diskUnknown}</p>
{/if}
