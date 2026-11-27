<script lang="ts">
	// What a proposal changes in one file, in plain words: never fact ids or front matter.
	import type { FileChangesView } from './api.ts';
	import { hrefOf } from './format.ts';
	import { i18n } from './i18n.svelte.ts';
	import { fileName, label, sides } from './proposal.ts';

	/** `heading` false in the inbox, where the proposal's title already names the file. */
	let { file, heading = true }: { file: FileChangesView; heading?: boolean } = $props();
	const href = $derived(hrefOf({ id: file.id, kind: file.kind }));
	const name = $derived(fileName(file));
</script>

<section class="diff-file">
	{#if heading}
		<h2>
			{#if href}<a {href}>{name}</a>{:else}{name}{/if}
			{#if file.created}<span class="badge">{i18n.t.newFile}</span>{/if}
		</h2>
	{/if}
	<ul class="diff">
		{#each file.changes as change, i (i)}
			{@const both = sides(change)}
			<li>
				<span class="what">{label(change)}</span>
				<div class="sides">
					{#if change.what === 'merged'}
						<p class="said">{i18n.t.mergedInto(change.before ?? '', change.after ?? file.name)}</p>
					{:else if change.what === 'links'}
						<p class="said">{i18n.t.linksRedirected(Number(change.after ?? 0))}</p>
					{/if}
					{#each both.removed as text, j (j)}
						<p class="removed">
							<span class="sign" aria-hidden="true">−</span>
							<span><span class="visually-hidden">{i18n.t.removed} : </span><del>{text}</del></span>
						</p>
					{/each}
					{#each both.added as text, j (j)}
						<p class="added">
							<span class="sign" aria-hidden="true">+</span>
							<span><span class="visually-hidden">{i18n.t.added} : </span><ins>{text}</ins></span>
						</p>
					{/each}
				</div>
			</li>
		{/each}
	</ul>
</section>
