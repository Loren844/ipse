<script lang="ts">
	import '@fontsource-variable/atkinson-hyperlegible-next';
	import '@fontsource-variable/bricolage-grotesque';
	import '../app.css';
	import { page } from '$app/state';
	import { onMount } from 'svelte';
	import { app } from '#lib/app.svelte.ts';
	import { i18n, type Language } from '#lib/i18n.svelte.ts';
	import { icons } from '#lib/icons.ts';
	import Mark from '#lib/Mark.svelte';

	let { children } = $props();

	onMount(() => {
		app.load();
	});

	$effect(() => {
		document.documentElement.lang = i18n.language;
	});

	const sections = $derived([
		{ href: '/', label: i18n.t.inbox, icon: icons.inbox },
		{ href: '/ais', label: i18n.t.ais, icon: icons.ais },
		{ href: '/history', label: i18n.t.history, icon: icons.history }
	]);

	/** Up to two initials of a name, for the owner's badge. */
	function initials(name: string): string {
		const words = name.split(/\s+/).filter(Boolean);
		const letters = words.length > 1 ? [words[0], words[words.length - 1]] : words;
		return letters.map((w) => w[0]?.toLocaleUpperCase() ?? '').join('');
	}

	const languages: { code: Language; label: string; name: string }[] = [
		{ code: 'fr', label: 'FR', name: 'Français' },
		{ code: 'en', label: 'EN', name: 'English' }
	];
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
			<nav aria-label={i18n.t.navLabel}>
				<ul>
					{#each sections as section (section.href)}
						{@const current = page.url.pathname === section.href}
						<li>
							<a href={section.href} aria-current={current ? 'page' : undefined}>
								<svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true"><path d={section.icon} /></svg>
								<span>{section.label}</span>
								{#if section.href === '/' && info.waiting > 0}
									<span class="count" aria-label={i18n.t.waiting(info.waiting)}>{info.waiting}</span>
								{/if}
							</a>
						</li>
					{/each}
				</ul>
			</nav>
			<div class="foot">
				<p class="owner">
					<span class="face" aria-hidden="true">{initials(info.name)}</span>
					<span class="who">
						<span class="of">{i18n.t.memoryOf}</span>
						<span class="name" data-testid="memory-name">{info.name}</span>
					</span>
				</p>
				<div class="languages" role="group" aria-label={i18n.t.language}>
					{#each languages as language (language.code)}
						<button
							type="button"
							lang={language.code}
							aria-pressed={i18n.language === language.code}
							title={language.name}
							onclick={() => i18n.choose(language.code)}>{language.label}</button
						>
					{/each}
				</div>
			</div>
		</header>
		<main id="content" tabindex="-1">
			{@render children()}
		</main>
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
