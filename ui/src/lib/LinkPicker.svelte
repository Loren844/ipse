<script lang="ts" module>
	/** A link the person chose: the value written in the file, and what it shows. */
	export type Picked = { value: string; target: string | null; name: string; id: string | null; kind: string | null };
</script>

<script lang="ts">
	// Finds a file of the memory to link to, as the person types; with `free`, a plain
	// name is kept as written. A combobox: arrows to move, Enter to choose, Escape to
	// close.
	import { api, type FoundView } from './api.ts';
	import { i18n } from './i18n.svelte.ts';
	import { iconOf } from './kinds.ts';

	let {
		id,
		label,
		free = false,
		kinds = null,
		onpick
	}: { id: string; label: string; free?: boolean; kinds?: string[] | null; onpick: (picked: Picked) => void } = $props();

	let text = $state('');
	let found: FoundView[] = $state([]);
	let open = $state(false);
	let active = $state(-1);
	let timer: ReturnType<typeof setTimeout> | undefined;
	let ticket = 0;

	const options = $derived([
		...found.map((f) => ({ key: f.id, found: f as FoundView | null, text: f.name })),
		...(free && text.trim() ? [{ key: '', found: null, text: text.trim() }] : [])
	]);

	function typed() {
		clearTimeout(timer);
		const q = text.trim();
		if (!q) {
			found = [];
			open = false;
			return;
		}
		timer = setTimeout(async () => {
			const mine = ++ticket;
			try {
				const view = await api.search(q);
				if (mine !== ticket) return;
				found = view.items.filter((f) => f.kind !== 'summary' && (!kinds || kinds.includes(f.kind))).slice(0, 6);
			} catch {
				found = [];
			}
			open = true;
			active = options.length ? 0 : -1;
		}, 120);
		open = true;
	}

	function choose(at: number) {
		const option = options[at];
		if (!option) return;
		const f = option.found;
		onpick(
			f
				? { value: `[[${f.stem}]]`, target: f.stem, name: f.name, id: f.id, kind: f.kind }
				: { value: option.text, target: null, name: option.text, id: null, kind: null }
		);
		text = '';
		found = [];
		open = false;
		active = -1;
	}

	function key(e: KeyboardEvent) {
		if (e.key === 'ArrowDown' && options.length) {
			e.preventDefault();
			open = true;
			active = (active + 1) % options.length;
		} else if (e.key === 'ArrowUp' && options.length) {
			e.preventDefault();
			active = (active - 1 + options.length) % options.length;
		} else if (e.key === 'Enter') {
			if (open && active >= 0) {
				e.preventDefault();
				choose(active);
			} else if (text.trim()) {
				e.preventDefault();
			}
		} else if (e.key === 'Escape' && open) {
			e.preventDefault();
			open = false;
		}
	}
</script>

<div class="picker">
	<input
		{id}
		type="text"
		role="combobox"
		autocomplete="off"
		spellcheck="false"
		aria-label={label}
		aria-expanded={open && options.length > 0}
		aria-controls="{id}-list"
		aria-activedescendant={open && active >= 0 ? `${id}-option-${active}` : undefined}
		placeholder={i18n.t.pickHint}
		bind:value={text}
		oninput={typed}
		onkeydown={key}
		onblur={() => setTimeout(() => (open = false), 150)}
	/>
	{#if open && options.length}
		<ul id="{id}-list" role="listbox" aria-label={label}>
			{#each options as option, i (option.key + i)}
				<li
					id="{id}-option-{i}"
					role="option"
					aria-selected={i === active}
					onmousedown={(e) => {
						e.preventDefault();
						choose(i);
					}}
				>
					{#if option.found}
						<svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true"><path d={iconOf(option.found.kind)} /></svg>
						<span>{option.text}</span>
						<span class="kind">{i18n.t.kind[option.found.kind] ?? ''}</span>
					{:else}
						<span>{i18n.t.keepAsText(option.text)}</span>
					{/if}
				</li>
			{/each}
		</ul>
	{/if}
</div>
