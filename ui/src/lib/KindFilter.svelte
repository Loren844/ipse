<script lang="ts">
	// One kind of event at a time (issue #134): every kind given, or all of them.
	import { i18n } from './i18n.svelte.ts';

	let { kinds, kind = $bindable(null) }: { kinds: string[]; kind: string | null } = $props();
	const name = (k: string) => i18n.t.eventKinds[k] ?? k;
	const sorted = $derived([...kinds].sort((a, b) => name(a).localeCompare(name(b), i18n.language)));
</script>

<div class="segmented kind-filter" role="group" aria-label={i18n.t.kindFilterLabel}>
	<button type="button" aria-pressed={kind === null} onclick={() => (kind = null)}>{i18n.t.allKinds}</button>
	{#each sorted as k (k)}
		<button type="button" aria-pressed={kind === k} onclick={() => (kind = k)}>{name(k)}</button>
	{/each}
</div>
