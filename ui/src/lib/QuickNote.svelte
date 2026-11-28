<script lang="ts">
	// A quick note, without an AI (issue #277): one line, saved at once as the person's,
	// with today's date. With `mentions`, `@` and a few letters find a person or a place
	// to link: the note keeps `@Name` while it is typed, and a link once saved.
	import { api, type FoundView } from './api.ts';
	import { i18n } from './i18n.svelte.ts';
	import { iconOf } from './kinds.ts';

	let {
		id,
		label,
		placeholder,
		mentions = false,
		save
	}: {
		id: string;
		label: string;
		placeholder: string;
		mentions?: boolean;
		/** Saves the note as written, its mentions as links; throws when it could not. */
		save: (text: string) => Promise<void>;
	} = $props();

	const MAX = 500;
	let text = $state('');
	let busy = $state(false);
	let input: HTMLInputElement | undefined = $state();
	/** The mentions chosen: what the line shows, and the link it stands for. */
	let picked: { shown: string; link: string }[] = $state([]);
	let found: FoundView[] = $state([]);
	let active = $state(-1);
	let asking: { from: number; to: number } | null = $state(null);
	let timer: ReturnType<typeof setTimeout> | undefined;
	let ticket = 0;
	const open = $derived(asking !== null && found.length > 0);

	/** The `@word` the caret is in, if any. */
	function typed() {
		if (!mentions || !input) return;
		const caret = input.selectionStart ?? text.length;
		const before = text.slice(0, caret);
		const match = /(^|\s)@([^\s@]{1,40})$/u.exec(before);
		clearTimeout(timer);
		if (!match) {
			asking = null;
			found = [];
			return;
		}
		const from = caret - match[2].length - 1;
		asking = { from, to: caret };
		const query = match[2];
		timer = setTimeout(async () => {
			const mine = ++ticket;
			try {
				const view = await api.search(query);
				if (mine !== ticket) return;
				found = view.groups
					.flatMap((g) => g.items)
					.filter((f) => f.kind === 'person' || f.kind === 'place')
					.slice(0, 6);
			} catch {
				found = [];
			}
			active = found.length ? 0 : -1;
		}, 120);
	}

	function choose(at: number) {
		const f = found[at];
		if (!f || !asking) return;
		const shown = `@${f.name}`;
		text = `${text.slice(0, asking.from)}${shown} ${text.slice(asking.to)}`;
		picked = [...picked.filter((p) => p.shown !== shown), { shown, link: `[[${f.stem}|${f.name}]]` }];
		const caret = asking.from + shown.length + 1;
		asking = null;
		found = [];
		// The caret goes after the mention now, before the next key is typed.
		if (input) {
			input.value = text;
			input.setSelectionRange(caret, caret);
		}
	}

	function key(e: KeyboardEvent) {
		if (!open) return;
		if (e.key === 'ArrowDown') {
			e.preventDefault();
			active = (active + 1) % found.length;
		} else if (e.key === 'ArrowUp') {
			e.preventDefault();
			active = (active - 1 + found.length) % found.length;
		} else if (e.key === 'Enter' || e.key === 'Tab') {
			if (active >= 0) {
				e.preventDefault();
				choose(active);
			}
		} else if (e.key === 'Escape') {
			e.preventDefault();
			asking = null;
			found = [];
		}
	}

	/** The line as saved: each mention still written as chosen becomes its link. */
	function linked(line: string): string {
		let out = line;
		for (const p of [...picked].sort((a, b) => b.shown.length - a.shown.length)) out = out.split(p.shown).join(p.link);
		return out;
	}

	async function submit(e: SubmitEvent) {
		e.preventDefault();
		const line = text.trim();
		if (!line || busy) return;
		busy = true;
		try {
			await save(linked(line));
			text = '';
			picked = [];
		} catch {
			// The page says what went wrong; the note stays to try again.
		}
		busy = false;
		input?.focus();
	}
</script>

<form class="quick-note" onsubmit={submit}>
	<div class="picker">
		<input
			{id}
			bind:this={input}
			type="text"
			role={mentions ? 'combobox' : undefined}
			autocomplete="off"
			aria-label={label}
			aria-expanded={mentions ? open : undefined}
			aria-controls={mentions ? `${id}-list` : undefined}
			aria-activedescendant={open && active >= 0 ? `${id}-option-${active}` : undefined}
			maxlength={MAX}
			{placeholder}
			bind:value={text}
			oninput={typed}
			onkeydown={key}
			onblur={() => setTimeout(() => (asking = null), 150)}
		/>
		{#if open}
			<ul id="{id}-list" role="listbox" aria-label={i18n.t.mentionLabel}>
				{#each found as f, i (f.id)}
					<li
						id="{id}-option-{i}"
						role="option"
						aria-selected={i === active}
						onmousedown={(e) => {
							e.preventDefault();
							choose(i);
						}}
					>
						<svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true"><path d={iconOf(f.kind)} /></svg>
						<span>{f.name}</span>
						<span class="kind">{i18n.t.kind[f.kind] ?? ''}</span>
					</li>
				{/each}
			</ul>
		{/if}
	</div>
	<button type="submit" class="button primary" disabled={busy || !text.trim()}>{i18n.t.noteSave}</button>
</form>

<style>
	.quick-note {
		display: flex;
		gap: 0.5rem;
		align-items: flex-start;
	}

	.quick-note input {
		box-sizing: border-box;
		width: 100%;
		font: inherit;
		color: var(--text);
		background: var(--surface);
		border: 1.5px solid var(--line-strong);
		border-radius: var(--radius-pill);
		padding: 0.6rem 1rem;
	}

	.quick-note input:focus {
		outline: none;
		border-color: var(--thread);
	}

	.quick-note .button {
		flex: none;
	}
</style>
