<script lang="ts">
	import '@fontsource-variable/atkinson-hyperlegible-next';
	import '@fontsource-variable/bricolage-grotesque';
	import '../app.css';
	import { page } from '$app/state';
	import { afterNavigate, goto } from '$app/navigation';
	import { onMount, tick } from 'svelte';
	import { app, inApp } from '#lib/app.svelte.ts';
	import { i18n } from '#lib/i18n.svelte.ts';
	import { icons } from '#lib/icons.ts';
	import OwnerMenu from '#lib/OwnerMenu.svelte';
	import UpdateBanner from '#lib/UpdateBanner.svelte';
	import Mark from '#lib/Mark.svelte';
	import { initials } from '#lib/format.ts';
	import { api, type SearchView } from '#lib/api.ts';
	import FoundItem from '#lib/FoundItem.svelte';

	let { children } = $props();

	/** One entry of the navigation. */
	type Section = { href: string; label: string; icon: string };

	onMount(() => {
		app.load();
		if (!inApp) return;
		// In the desktop app (#167): the interface is not a page to select or drag
		// from. Files dropped on the window still arrive (Sources).
		document.documentElement.dataset.app = '';
		const stopDrag = (e: DragEvent) => {
			if (e.target instanceof Element && e.target.closest('a, img, svg')) e.preventDefault();
		};
		document.addEventListener('dragstart', stopDrag);
		return () => document.removeEventListener('dragstart', stopDrag);
	});

	$effect(() => {
		document.documentElement.lang = i18n.language;
	});

	// The side bar in three (issue #265): what the person looks at; what waits for
	// them; what they manage, at the bottom by their name. The graph is reached from a
	// file (#244), the settings from the menu under their name (#176).
	const look = $derived([
		{ href: '/', label: i18n.t.home, icon: icons.home },
		{ href: '/memory', label: i18n.t.memory, icon: icons.memory },
		{ href: '/agenda', label: i18n.t.agenda, icon: icons.agenda },
		{ href: '/map', label: i18n.t.map, icon: icons.place },
		{ href: '/summaries', label: i18n.t.summariesNav, icon: icons.summaries }
	]);
	const review = $derived({ href: '/review', label: i18n.t.inbox, icon: icons.inbox });
	const manage = $derived([
		{ href: '/ais', label: i18n.t.ais, icon: icons.ais },
		{ href: '/sources', label: i18n.t.sourcesNav, icon: icons.sources },
		{ href: '/history', label: i18n.t.history, icon: icons.history }
	]);

	/** Réglages lives in the menu under the owner's name (#176), and behind More on a phone. */
	const settings = $derived({ href: '/settings', label: i18n.t.settings, icon: icons.settings });

	// On a phone, the four sections used most stay in the bottom bar; the others wait
	// behind More (issue #140), grouped as in the side bar.
	const MAIN = ['/', '/memory', '/agenda', '/review'];
	const sections = $derived([...look, review]);
	const main = $derived(MAIN.map((href) => sections.find((s) => s.href === href)).filter((s) => s !== undefined));
	// More starts with the owner's file (#176), then what else there is to look at.
	const moreLook = $derived([
		...(app.state.kind === 'ready' && app.state.info.me
			? [{ href: `/people/${app.state.info.me}`, label: i18n.t.yourFile, icon: icons.people }]
			: []),
		...look.filter((s) => !MAIN.includes(s.href))
	]);
	const moreManage = $derived([...manage, settings]);
	const others = $derived([...moreLook, ...moreManage]);
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

	/** The rail's layout, wider than a phone (the breakpoint of app.css). */
	const DESKTOP = '(min-width: 761px)';

	// Widening the window to the desktop layout hides the bottom bar: its panel goes
	// with it, or the page would stay blocked behind it (#177).
	$effect(() => {
		const wide = window.matchMedia(DESKTOP);
		const change = () => {
			if (wide.matches) closeMore();
		};
		wide.addEventListener('change', change);
		return () => wide.removeEventListener('change', change);
	});

	let railQuery = $state('');
	// Results while typing (issue #258): a few of each kind, Enter for all of them.
	let railFound: SearchView | null = $state(null);
	let railTimer: ReturnType<typeof setTimeout> | undefined;
	let railTicket = 0;

	function railTyped() {
		clearTimeout(railTimer);
		const q = railQuery.trim();
		if (!q) {
			closeRail();
			return;
		}
		railTimer = setTimeout(async () => {
			const mine = ++railTicket;
			try {
				const view = await api.search(q, true);
				if (mine === railTicket && railQuery.trim()) railFound = view;
			} catch {
				if (mine === railTicket) railFound = null;
			}
		}, 80);
	}

	function closeRail() {
		clearTimeout(railTimer);
		railTicket++;
		railFound = null;
	}

	/** Searches the whole memory, from any screen. */
	function searchMemory(e: SubmitEvent) {
		e.preventDefault();
		const q = railQuery.trim();
		if (!q) return;
		railQuery = '';
		closeRail();
		goto(`/memory?q=${encodeURIComponent(q)}`);
	}

	// A result opened, or another screen: the search box starts empty again.
	afterNavigate(() => {
		railQuery = '';
		closeRail();
	});

	/** The section a page belongs to: a person's page is under Memory. */
	function isCurrent(href: string): boolean {
		const path = page.url.pathname;
		// Around a file and its graph are reached from the memory's files (issue #244).
		if (href === '/memory' && (path === '/around' || path === '/graph')) return true;
		return href === '/' ? path === '/' : path === href || path.startsWith(`${href}/`);
	}

</script>

{#snippet railLink(section: Section, info: { waiting: number; failing: number })}
	<a href={section.href} aria-current={isCurrent(section.href) ? 'page' : undefined}>
		<svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true"><path d={section.icon} /></svg>
		<span>{section.label}</span>
		{#if section.href === '/review' && info.waiting > 0}
			<span class="count" aria-label={i18n.t.waiting(info.waiting)}>{info.waiting}</span>
		{/if}
		{#if section.href === '/sources' && info.failing > 0}
			<span class="count warn" role="img" aria-label={i18n.t.sourcesFailing(info.failing)}>!</span>
		{/if}
	</a>
{/snippet}

{#snippet moreLink(section: Section, info: { failing: number })}
	<a href={section.href} aria-current={isCurrent(section.href) ? 'page' : undefined} onclick={closeMore}>
		<svg viewBox="0 0 24 24" width="20" height="20" aria-hidden="true"><path d={section.icon} /></svg>
		<span>{section.label}</span>
		{#if section.href === '/sources' && info.failing > 0}
			<span class="count warn" role="img" aria-label={i18n.t.sourcesFailing(info.failing)}>!</span>
		{/if}
	</a>
{/snippet}

<svelte:window
	onkeydown={(e) => {
		// Escape closes the results while typing, back in the search box.
		if (e.key === 'Escape' && railFound) {
			closeRail();
			document.getElementById('rail-search')?.focus();
		}
	}}
/>

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
			<div
				class="rail-search-box"
				onfocusout={(e) => {
					if (!(e.relatedTarget instanceof Node && e.currentTarget.contains(e.relatedTarget))) closeRail();
				}}
			>
				<form class="rail-search" role="search" onsubmit={searchMemory}>
					<label for="rail-search" class="visually-hidden">{i18n.t.searchMemory}</label>
					<svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true"><path d={icons.search} /></svg>
					<input
						id="rail-search"
						type="search"
						placeholder={i18n.t.searchShort}
						bind:value={railQuery}
						oninput={railTyped}
						autocomplete="off"
						spellcheck="false"
						aria-controls="rail-found"
					/>
				</form>
				{#if railFound}
					<div class="rail-found" id="rail-found" role="region" aria-label={i18n.t.foundLabel} data-testid="rail-found">
						{#if railFound.total}
							{#each railFound.groups as group (group.kind)}
								<p class="rail-found-kind">{i18n.t.kinds[group.kind] ?? group.kind} <span class="found-count">{group.count}</span></p>
								<ul class="people found">
									{#each group.items as item (item.id)}
										<li><FoundItem {item} compact /></li>
									{/each}
								</ul>
							{/each}
							<a class="see-all" href="/memory?q={encodeURIComponent(railQuery.trim())}">{i18n.t.seeAllFound(railFound.total)}</a>
						{:else}
							<p class="quiet">{i18n.t.nothingFound}</p>
						{/if}
					</div>
				{/if}
			</div>
			<nav aria-label={i18n.t.navLabel}>
				<ul>
					{#each look as section (section.href)}
						<li>{@render railLink(section, info)}</li>
					{/each}
				</ul>
				<ul>
					<li>{@render railLink(review, info)}</li>
				</ul>
				<p class="nav-group" id="nav-manage">{i18n.t.manage}</p>
				<ul class="manage" aria-labelledby="nav-manage">
					{#each manage as section (section.href)}
						<li>{@render railLink(section, info)}</li>
					{/each}
				</ul>
			</nav>
			<div class="foot">
				<OwnerMenu name={info.name} me={info.me} />
			</div>
		</header>
		<main id="content" tabindex="-1">
			{#if inApp}<UpdateBanner />{/if}
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
						<span class="face" aria-hidden="true">{initials(info.name)}</span>
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
				// On a phone, back to More; on the desktop layout, More is hidden: the rail.
				if (window.matchMedia(DESKTOP).matches) document.querySelector<HTMLElement>('.rail nav a')?.focus();
				else moreButton?.focus();
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
				{#each moreLook as section (section.href)}
					<li>{@render moreLink(section, info)}</li>
				{/each}
			</ul>
			<p class="nav-group" id="more-manage">{i18n.t.manage}</p>
			<ul aria-labelledby="more-manage">
				{#each moreManage as section (section.href)}
					<li>{@render moreLink(section, info)}</li>
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
			<p>{inApp ? i18n.t.endedHintApp : i18n.t.endedHint}</p>
		{:else}
			<h1>{i18n.t.failed}</h1>
			<p>{inApp ? i18n.t.failedHintApp : i18n.t.failedHint}</p>
		{/if}
	</main>
{/if}
