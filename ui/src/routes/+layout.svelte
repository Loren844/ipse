<script lang="ts">
	import '@fontsource-variable/atkinson-hyperlegible-next';
	import '@fontsource-variable/bricolage-grotesque';
	import '../app.css';
	import { page } from '$app/state';
	import { afterNavigate, goto } from '$app/navigation';
	import { onMount, tick } from 'svelte';
	import { app } from '#lib/app.svelte.ts';
	import { i18n } from '#lib/i18n.svelte.ts';
	import { icons } from '#lib/icons.ts';
	import Mark from '#lib/Mark.svelte';
	import { initials } from '#lib/format.ts';

	let { children } = $props();

	onMount(() => {
		app.load();
	});

	$effect(() => {
		document.documentElement.lang = i18n.language;
	});

	const groups = $derived([
		[
			{ href: '/', label: i18n.t.home, icon: icons.home },
			{ href: '/memory', label: i18n.t.memory, icon: icons.memory },
			{ href: '/graph', label: i18n.t.graph, icon: icons.graph },
			{ href: '/agenda', label: i18n.t.agenda, icon: icons.agenda },
			{ href: '/map', label: i18n.t.map, icon: icons.place },
			{ href: '/summaries', label: i18n.t.summariesNav, icon: icons.summaries }
		],
		[
			{ href: '/review', label: i18n.t.inbox, icon: icons.inbox },
			{ href: '/ais', label: i18n.t.ais, icon: icons.ais },
			{ href: '/history', label: i18n.t.history, icon: icons.history },
			{ href: '/settings', label: i18n.t.settings, icon: icons.settings }
		]
	]);

	// On a phone, the four sections used most stay in the bottom bar; the others wait
	// behind More (issue #140).
	const MAIN = ['/', '/memory', '/agenda', '/review'];
	const sections = $derived(groups.flat());
	const main = $derived(MAIN.map((href) => sections.find((s) => s.href === href)).filter((s) => s !== undefined));
	const others = $derived(sections.filter((s) => !MAIN.includes(s.href)));
	const inOthers = $derived(others.find((s) => isCurrent(s.href)) ?? null);

	let sheet: HTMLDialogElement | undefined = $state();
	let moreButton: HTMLButtonElement | undefined = $state();
	let sheetOpen = $state(false);

	async function openMore() {
		sheet?.showModal();
		sheetOpen = true;
		await tick();
		(sheet?.querySelector<HTMLElement>('a[aria-current="page"]') ?? sheet?.querySelector<HTMLElement>('ul a'))?.focus();
	}

	function closeMore() {
		if (sheet?.open) sheet.close();
	}

	afterNavigate(() => closeMore());

	let railQuery = $state('');

	/** Searches the whole memory, from any screen. */
	function searchMemory(e: SubmitEvent) {
		e.preventDefault();
		const q = railQuery.trim();
		if (!q) return;
		railQuery = '';
		goto(`/memory?q=${encodeURIComponent(q)}`);
	}

	/** The section a page belongs to: a person's page is under Memory. */
	function isCurrent(href: string): boolean {
		const path = page.url.pathname;
		return href === '/' ? path === '/' : path === href || path.startsWith(`${href}/`);
	}

</script>

<svelte:head>
	<title>{app.state.kind === 'ready' ? `${app.state.info.name} · Ipse` : 'Ipse'}</title>
</svelte:head>

{#if app.state.kind === 'ready'}
	{@const info = app.state.info}
	<div class="frame">
		<header class="rail">
			<a class="brand" href="/" aria-label={i18n.t.appName}>
				<Mark size={30} />
				<span class="wordmark">ipse</span>
			</a>
			<form class="rail-search" role="search" onsubmit={searchMemory}>
				<label for="rail-search" class="visually-hidden">{i18n.t.searchMemory}</label>
				<svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true"><path d={icons.search} /></svg>
				<input id="rail-search" type="search" placeholder={i18n.t.searchShort} bind:value={railQuery} autocomplete="off" spellcheck="false" />
			</form>
			<nav aria-label={i18n.t.navLabel}>
				{#each groups as group, g (g)}
					<ul>
						{#each group as section (section.href)}
							{@const current = isCurrent(section.href)}
							<li>
								<a href={section.href} aria-current={current ? 'page' : undefined}>
									<svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true"><path d={section.icon} /></svg>
									<span>{section.label}</span>
									{#if section.href === '/review' && info.waiting > 0}
										<span class="count" aria-label={i18n.t.waiting(info.waiting)}>{info.waiting}</span>
									{/if}
								</a>
							</li>
						{/each}
					</ul>
				{/each}
			</nav>
			<div class="foot">
				<p class="owner">
					<span class="face" aria-hidden="true">{initials(info.name)}</span>
					<span class="who">
						<span class="of">{i18n.t.memoryOf}</span>
						<span class="name" data-testid="memory-name">{info.name}</span>
					</span>
				</p>
			</div>
		</header>
		<main id="content" tabindex="-1">
			{@render children()}
		</main>
		<nav class="tabbar" aria-label={i18n.t.navLabel}>
			<ul>
				{#each main as section (section.href)}
					{@const current = isCurrent(section.href)}
					<li>
						<a href={section.href} aria-current={current ? 'page' : undefined}>
							<svg viewBox="0 0 24 24" width="22" height="22" aria-hidden="true"><path d={section.icon} /></svg>
							<span>{section.label}</span>
							{#if section.href === '/review' && info.waiting > 0}
								<span class="count" aria-label={i18n.t.waiting(info.waiting)}>{info.waiting}</span>
							{/if}
						</a>
					</li>
				{/each}
				<li>
					<button
						type="button"
						class:current={inOthers !== null}
						aria-haspopup="dialog"
						aria-expanded={sheetOpen}
						aria-describedby={inOthers ? 'more-current' : undefined}
						bind:this={moreButton}
						onclick={openMore}
					>
						<svg viewBox="0 0 24 24" width="22" height="22" aria-hidden="true"><path d={icons.more} /></svg>
						<span>{i18n.t.more}</span>
					</button>
					{#if inOthers}<span id="more-current" class="visually-hidden">{i18n.t.youAreIn(inOthers.label)}</span>{/if}
				</li>
			</ul>
		</nav>
		<dialog
			class="more"
			aria-labelledby="more-title"
			bind:this={sheet}
			onclose={() => {
				sheetOpen = false;
				moreButton?.focus();
			}}
			onclick={(e) => {
				// A tap on the backdrop, outside the panel, closes it.
				if (e.target === sheet) closeMore();
			}}
		>
			<div class="sheet-head">
				<h2 id="more-title">{i18n.t.moreSections}</h2>
				<button type="button" class="remove" aria-label={i18n.t.closePanel} onclick={closeMore}
					><svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true"><path d="M6 6l12 12M18 6 6 18" /></svg></button
				>
			</div>
			<ul>
				{#each others as section (section.href)}
					<li>
						<a href={section.href} aria-current={isCurrent(section.href) ? 'page' : undefined} onclick={closeMore}>
							<svg viewBox="0 0 24 24" width="20" height="20" aria-hidden="true"><path d={section.icon} /></svg>
							<span>{section.label}</span>
						</a>
					</li>
				{/each}
			</ul>
			<p class="owner">
				<span class="face" aria-hidden="true">{initials(info.name)}</span>
				<span class="who">
					<span class="of">{i18n.t.memoryOf}</span>
					<span class="name">{info.name}</span>
				</span>
			</p>
		</dialog>
	</div>
{:else}
	<main class="alone" aria-live="polite">
		{#if app.state.kind === 'loading'}
			<p class="quiet">{i18n.t.loading}</p>
		{:else if app.state.kind === 'ended'}
			<h1>{i18n.t.ended}</h1>
			<p>{i18n.t.endedHint}</p>
		{:else}
			<h1>{i18n.t.failed}</h1>
			<p>{i18n.t.failedHint}</p>
		{/if}
	</main>
{/if}
