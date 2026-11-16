<script lang="ts">
	// What a proposal changes in one file, in plain words: never fact ids or front matter.
	import type { ChangeView, FileChangesView } from './api.ts';
	import { birthday, hrefOf, isDay, periodName, phone, when } from './format.ts';
	import { i18n } from './i18n.svelte.ts';

	let { file }: { file: FileChangesView } = $props();
	const href = $derived(hrefOf({ id: file.id, kind: file.kind }));
	const name = $derived(file.kind === 'summary' ? i18n.t.summaryOf(periodName(file.name)) : file.name);

	function label(change: ChangeView): string {
		if (change.what === 'key') return i18n.t.keys[change.key ?? ''] ?? change.key ?? '';
		if (change.what === 'fact') return i18n.t.fact;
		if (change.what === 'relation') return i18n.t.relation;
		if (change.what === 'summary') return i18n.t.summaryLabel;
		return i18n.t.notesLabel;
	}

	/** Keys holding a list: only the items that come and go are shown. */
	const LISTS = new Set(['phones', 'emails', 'aliases', 'participants']);

	type Side = { removed: string[]; added: string[] };
	function sides(change: ChangeView): Side {
		const before = change.before === null ? [] : [shown(change, change.before)];
		const after = change.after === null ? [] : [shown(change, change.after)];
		if (change.what !== 'key' || !LISTS.has(change.key ?? '')) return { removed: before, added: after };
		const items = (text: string | null) => (text === null ? [] : text.split(', ').map((t) => shown(change, t)));
		const was = items(change.before);
		const is = items(change.after);
		return { removed: was.filter((t) => !is.includes(t)), added: is.filter((t) => !was.includes(t)) };
	}

	/** A key's value as the file's own page shows it. */
	function shown(change: ChangeView, text: string): string {
		if (change.what !== 'key') return text;
		if (change.key === 'phones') return text.split(', ').map(phone).join(', ');
		if (change.key === 'birthday') return birthday(text);
		if ((change.key === 'start' || change.key === 'end') && !isDay(text) && !Number.isNaN(Date.parse(text))) return when(text);
		return text;
	}
</script>

<section class="diff-file">
	<h2>
		{#if href}<a {href}>{name}</a>{:else}{name}{/if}
		{#if file.created}<span class="badge">{i18n.t.newFile}</span>{/if}
	</h2>
	<ul class="diff">
		{#each file.changes as change, i (i)}
			{@const both = sides(change)}
			<li>
				<span class="what">{label(change)}</span>
				<div class="sides">
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
