<script lang="ts">
	import { onMount } from 'svelte';
	import { api, type HomeView } from '#lib/api.ts';
	import Events from '#lib/Events.svelte';
	import DiskNote from '#lib/DiskNote.svelte';
	import FirstSteps from '#lib/FirstSteps.svelte';
	import { longDay } from '#lib/format.ts';
	import { i18n } from '#lib/i18n.svelte.ts';
	import Mention from '#lib/Mention.svelte';
	import Now from '#lib/Now.svelte';
	import SummaryCard from '#lib/SummaryCard.svelte';
	import { Loader } from '#lib/remote.svelte.ts';

	const home = new Loader<HomeView>();
	onMount(() => home.load(api.home));
</script>

{#if home.state.kind === 'ready'}
	{@const view = home.state.value}
	<div class="screen">
		<header class="page-head">
			<p class="date">{longDay(view.today)}</p>
			<h1 id="coming">{i18n.t.comingUp}</h1>
		</header>

		{#if view.waiting > 0 || view.questions > 0}
			<a class="banner" href="/review">
				<span>{i18n.t.waitingBanner(view.waiting, view.questions)}</span>
				<span class="action">{i18n.t.review}</span>
			</a>
		{/if}

		{#each view.automatic as a (a.standing)}
			<p class="automatic" data-testid="automatic">
				{i18n.t.automaticHome(a.count, a.standing)}
				<a href="/review#decided">{i18n.t.see}</a>
			</p>
		{/each}

		<DiskNote />
		{#if view.guide}
			<FirstSteps guide={view.guide} />
		{/if}

		<!-- The screen's title names this first section: no second heading repeats it. -->
		<section aria-labelledby="coming">
			{#if view.upcoming.length}
				<Events events={view.upcoming} today={view.today} group />
			{:else}
				<div class="empty-thread">
					<p class="lead">{i18n.t.nothingComing}</p>
					<p>{i18n.t.nothingComingHint}</p>
				</div>
			{/if}
		</section>

		{#if view.now.length}
			<section aria-labelledby="now" class="home-now">
				<h2 id="now">{i18n.t.nowTitle}</h2>
				<Now states={view.now} today={view.today} />
			</section>
		{/if}

		{#if view.latest}
			<section aria-labelledby="latest" class="latest-summary">
				<h2 id="latest">{i18n.t.latestSummary}</h2>
				<SummaryCard summary={view.latest} today={view.today} level={3} />
				<p class="quiet-link"><a href="/summaries">{i18n.t.allSummaries}</a></p>
			</section>
		{/if}

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
