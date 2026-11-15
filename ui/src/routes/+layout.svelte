<script lang="ts">
	import '@fontsource-variable/atkinson-hyperlegible-next';
	import '../app.css';
	import { page } from '$app/state';
	import { onMount } from 'svelte';
	import { app } from '#lib/app.svelte.ts';
	import { i18n, type Language } from '#lib/i18n.svelte.ts';

	let { children } = $props();

	onMount(() => {
		app.load();
	});

	$effect(() => {
		document.documentElement.lang = i18n.language;
	});

	const sections = $derived([
		{ href: '/', label: i18n.t.inbox },
		{ href: '/ais', label: i18n.t.ais },
		{ href: '/history', label: i18n.t.history }
	]);

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
			<a class="mark" href="/" aria-label={i18n.t.appName}>
				<svg viewBox="0 0 32 32" aria-hidden="true"><circle cx="16" cy="16" r="9" /></svg>
				<span>Ipse</span>
			</a>
			<p class="owner">
				<span class="of">{i18n.t.memoryOf}</span>
				<span class="name" data-testid="memory-name">{info.name}</span>
			</p>
			<nav aria-label={i18n.t.navLabel}>
				<ul>
					{#each sections as section (section.href)}
						{@const current = page.url.pathname === section.href}
						<li>
							<a href={section.href} aria-current={current ? 'page' : undefined}>
								<span>{section.label}</span>
								{#if section.href === '/' && info.waiting > 0}
									<span class="count" aria-label={i18n.t.waiting(info.waiting)}>{info.waiting}</span>
								{/if}
							</a>
						</li>
					{/each}
				</ul>
			</nav>
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
