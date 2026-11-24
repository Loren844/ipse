<script lang="ts">
	import { drop } from './motion.ts';
	// "Mémoire de …" at the bottom of the rail opens a small menu (issue #176): the
	// owner's file, the settings and, in the app, another memory. A menu button:
	// Enter or a click opens it on its first item, arrows move, Escape closes it.
	import { tick } from 'svelte';
	import { appLinks, inApp } from './app.svelte.ts';
	import { initials } from './format.ts';
	import { i18n } from './i18n.svelte.ts';

	let { name, me }: { name: string; me: string | null } = $props();
	let open = $state(false);
	let button: HTMLButtonElement | undefined = $state();
	let menu: HTMLUListElement | undefined = $state();

	const items = $derived(
		[
			me ? { href: `/people/${me}`, label: i18n.t.yourFile, reload: false } : null,
			{ href: '/settings', label: i18n.t.settings, reload: false },
			{ href: '/how', label: i18n.t.how.menu, reload: false },
			inApp ? { href: appLinks.switchMemory, label: i18n.t.switchMemory, reload: true } : null
		].filter((i) => i !== null)
	);

	const links = () => [...(menu?.querySelectorAll<HTMLAnchorElement>('a') ?? [])];

	async function show(focus: 'first' | 'last' = 'first') {
		open = true;
		await tick();
		const all = links();
		(focus === 'first' ? all[0] : all[all.length - 1])?.focus();
	}

	function hide(back = true) {
		open = false;
		if (back) button?.focus();
	}

	function keys(e: KeyboardEvent) {
		const all = links();
		const at = all.indexOf(document.activeElement as HTMLAnchorElement);
		if (e.key === 'Escape') {
			e.preventDefault();
			hide();
		} else if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
			e.preventDefault();
			const step = e.key === 'ArrowDown' ? 1 : -1;
			all[(at + step + all.length) % all.length]?.focus();
		} else if (e.key === 'Home' || e.key === 'End') {
			e.preventDefault();
			(e.key === 'Home' ? all[0] : all[all.length - 1])?.focus();
		} else if (e.key === 'Tab') {
			hide(false);
		}
	}

	// A click anywhere else closes it.
	$effect(() => {
		if (!open) return;
		const outside = (e: PointerEvent) => {
			if (!(e.target instanceof Node) || (!menu?.contains(e.target) && !button?.contains(e.target))) hide(false);
		};
		document.addEventListener('pointerdown', outside);
		return () => document.removeEventListener('pointerdown', outside);
	});
</script>

<div class="owner-menu">
	{#if open}
		<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
		<ul class="owner-items" transition:drop role="menu" aria-label={i18n.t.memoryOf} bind:this={menu} onkeydown={keys}>
			{#each items as item (item.href)}
				<li role="none">
					<a role="menuitem" href={item.href} data-sveltekit-reload={item.reload ? '' : undefined} onclick={() => hide(false)}>{item.label}</a>
				</li>
			{/each}
		</ul>
	{/if}
	<button
		type="button"
		class="owner"
		aria-haspopup="menu"
		aria-expanded={open}
		bind:this={button}
		onclick={() => (open ? hide() : show())}
		onkeydown={(e) => {
			if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
				e.preventDefault();
				show(e.key === 'ArrowDown' ? 'first' : 'last');
			}
		}}
	>
		<span class="face" aria-hidden="true">{initials(name)}</span>
		<span class="who">
			<span class="of">{i18n.t.memoryOf}</span>
			<span class="name" data-testid="memory-name">{name}</span>
		</span>
		<svg class="chevron" viewBox="0 0 24 24" width="16" height="16" aria-hidden="true"><path d="M6 9l6 6 6-6" /></svg>
	</button>
</div>
