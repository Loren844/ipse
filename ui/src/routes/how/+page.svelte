<script lang="ts">
	// How Ipse works (issue #172), in four knots on the thread, each with a small drawing:
	// the memory is a folder of one's own, sources fill it, each AI reads what it was
	// opened, an AI proposes and the person decides.
	import { api, type SettingsView } from '#lib/api.ts';
	import { i18n } from '#lib/i18n.svelte.ts';
	import { Loader } from '#lib/remote.svelte.ts';

	const settings = new Loader<SettingsView>();
	settings.load(() => api.settings());

	const GUIDE = 'https://github.com/Loren844/ipse/blob/main/docs/guide/getting-started.md';
	const parts = ['folder', 'sources', 'rights', 'proposals'] as const;
	const links: Record<(typeof parts)[number], string> = {
		folder: '/memory',
		sources: '/sources',
		rights: '/ais',
		proposals: '/review'
	};
</script>

<svelte:head><title>{i18n.t.how.title} · Ipse</title></svelte:head>

<div class="screen how">
	<header class="page-head">
		<h1>{i18n.t.how.title}</h1>
		<p class="intro">{i18n.t.how.lead}</p>
	</header>

	<ol class="how-parts">
		{#each parts as part (part)}
			{@const words = i18n.t.how[part]}
			<li data-part={part}>
				<svg class="how-drawing" viewBox="0 0 200 64" aria-hidden="true" focusable="false">
					{#if part === 'folder'}
						<!-- A folder of one's own, holding a file per person, place or event. -->
						<path d="M4 18a3 3 0 0 1 3-3h16l5 5h25a3 3 0 0 1 3 3v24a3 3 0 0 1-3 3H7a3 3 0 0 1-3-3z" />
						<path d="M64 32h18M77 27l5 5-5 5" />
						<path d="M92 14h16l6 6v30H92zM108 14v6h6M96 28h14M96 34h14M96 40h9" />
						<path d="M124 14h16l6 6v30h-22zM140 14v6h6M128 28h14M128 34h14M128 40h9" />
						<path d="M156 14h16l6 6v30h-22zM172 14v6h6M160 28h14M160 34h9" class="faint" />
					{:else if part === 'sources'}
						<!-- Contacts, calendars and places flow into the memory, a knot on the thread. -->
						<circle cx="16" cy="22" r="5" />
						<path d="M7 41c1.5-5 5-7 9-7s7.5 2 9 7" />
						<path d="M40 20h20a2 2 0 0 1 2 2v18a2 2 0 0 1-2 2H40a2 2 0 0 1-2-2V22a2 2 0 0 1 2-2zM38 27h24M44 16v6M56 16v6" />
						<path d="M82 44s-8-7.5-8-13a8 8 0 0 1 16 0c0 5.5-8 13-8 13z" />
						<circle cx="82" cy="31" r="2.5" />
						<path d="M102 32h22M119 27l5 5-5 5" />
						<circle cx="148" cy="32" r="13" class="knot" />
						<path d="M167 32h29" class="thread" />
					{:else if part === 'rights'}
						<!-- The memory gives an AI what it may read; a closed category stays shut. -->
						<circle cx="20" cy="32" r="13" class="knot" />
						<path d="M42 32h30M67 27l5 5-5 5" />
						<circle cx="96" cy="32" r="13" class="ring" />
						<path d="M91 37.5l5-11 5 11M92.8 33.5h6.4" />
						<path d="M144 30v-5a7 7 0 0 1 14 0v5M140 30h22v15h-22z" class="faint" />
					{:else}
						<!-- An AI proposes a change; the person accepts or refuses it. -->
						<circle cx="20" cy="32" r="13" class="ring" />
						<path d="M15 37.5l5-11 5 11M16.8 33.5h6.4" />
						<path d="M42 32h18M55 27l5 5-5 5" />
						<path d="M70 14h16l6 6v30H70zM86 14v6h6M75 33h12M81 27v12" class="added" />
						<path d="M102 32h18M115 27l5 5-5 5" />
						<circle cx="142" cy="19" r="10" />
						<path d="M137 19l3.5 3.5 6.5-7" />
						<circle cx="142" cy="45" r="10" class="faint" />
						<path d="M138 41l8 8M146 41l-8 8" class="faint" />
					{/if}
				</svg>
				<h2>{words.title}</h2>
				<p>{words.text}</p>
				{#if part === 'folder' && settings.state.kind === 'ready'}
					<p class="how-folder"><span>{i18n.t.how.yourFolder}</span> <code>{settings.state.value.folder}</code></p>
				{/if}
				<p><a href={links[part]}>{words.action}</a></p>
			</li>
		{/each}
	</ol>

	<p class="quiet-link"><a href={GUIDE}>{i18n.t.how.more}</a></p>
</div>
