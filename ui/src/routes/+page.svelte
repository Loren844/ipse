<script lang="ts">
	import { onMount } from 'svelte';
	import { api, type HomeView } from '#lib/api.ts';
	import Events from '#lib/Events.svelte';
	import { longDay } from '#lib/format.ts';
	import { i18n } from '#lib/i18n.svelte.ts';
	import Mention from '#lib/Mention.svelte';
	import { Loader } from '#lib/remote.svelte.ts';

	const home = new Loader<HomeView>();
	onMount(() => home.load(api.home));
</script>

{#if home.state.kind === 'ready'}
	{@const view = home.state.value}
	<div class="screen">
		<header class="page-head">
			<p class="date">{longDay(view.today)}</p>
			<h1>{i18n.t.comingUp}</h1>
		</header>

		{#if view.waiting > 0}
			<a class="banner" href="/review">
				<span>{i18n.t.waitingBanner(view.waiting)}</span>
				<span class="action">{i18n.t.review}</span>
			</a>
		{/if}

		<section aria-labelledby="coming">
			<h2 id="coming" class="visually-hidden">{i18n.t.comingUp}</h2>
			{#if view.upcoming.length}
				<Events events={view.upcoming} today={view.today} />
			{:else}
				<div class="empty-thread">
					<p class="lead">{i18n.t.nothingComing}</p>
					<p>{i18n.t.nothingComingHint}</p>
				</div>
			{/if}
		</section>

		{#if view.recent.length}
			<section aria-labelledby="recent">
				<h2 id="recent">{i18n.t.seenRecently}</h2>
				<p class="mentions">{#each view.recent as person, i (i)}<Mention link={person} />{/each}</p>
			</section>
		{/if}

		{#if view.me}
			<p class="quiet-link"><a href="/people/{view.me.id}">{i18n.t.yourFile}</a></p>
		{/if}
	</div>
{:else if home.state.kind === 'failed'}
	<p class="quiet">{i18n.t.failed}</p>
{/if}
