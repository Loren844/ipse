<script lang="ts">
	// A search's results by kind (issue #258), each kind with its count, the kind with
	// the best match first; events newest first.
	import type { SearchView } from './api.ts';
	import FoundItem from './FoundItem.svelte';
	import { i18n } from './i18n.svelte.ts';
	import { folderOf } from './kinds.ts';

	let { view, query }: { view: SearchView; query: string } = $props();
</script>

<div class="found-groups" data-testid="found">
	{#each view.groups as group (group.kind)}
		{@const folder = folderOf(group.kind)}
		<section class="found-group" aria-labelledby="found-{group.kind}" data-kind={group.kind}>
			<h2 id="found-{group.kind}">
				{i18n.t.kinds[group.kind] ?? group.kind}
				<span class="found-count">{group.count}</span>
			</h2>
			<ul class="people found">
				{#each group.items as item (item.id)}
					<li><FoundItem {item} /></li>
				{/each}
			</ul>
			{#if group.count > group.items.length}
				{#if folder}
					<a class="see-all" href="/memory/{folder}?q={encodeURIComponent(query)}">{i18n.t.seeAllKind(group.count)}</a>
				{:else}
					<p class="hint">{i18n.t.firstOf(group.items.length, group.count)}</p>
				{/if}
			{/if}
		</section>
	{/each}
</div>
