<script lang="ts">
	// The note of one day (issue #277), opened from a mention or a search.
	import { page } from '$app/state';
	import { api, NotFound, type DayNoteView } from '#lib/api.ts';
	import DayNote from '#lib/DayNote.svelte';
	import { dayOf, longDay } from '#lib/format.ts';
	import { i18n } from '#lib/i18n.svelte.ts';

	const day = $derived(page.params.day ?? '');
	const today = dayOf(new Date().toISOString());
	let note: DayNoteView | null = $state(null);
	let ready = $state(false);
	$effect(() => {
		const asked = day;
		ready = false;
		api.dayNote(asked)
			.then((n) => (note = n))
			.catch((error) => {
				note = null;
				if (!(error instanceof NotFound)) throw error;
			})
			.finally(() => (ready = asked === day));
	});
</script>

<svelte:head><title>{i18n.t.dayNoteOf(longDay(day))} · Ipse</title></svelte:head>

<article class="screen">
	<header class="page-head">
		<p class="date">{day === today ? i18n.t.dayNoteToday : i18n.t.dayNoteTitle}</p>
		<h1>{longDay(day)}</h1>
	</header>
	{#if ready}
		<DayNote bind:note editable={day === today} />
	{/if}
</article>
