<script lang="ts">
	// One search result, saying what it is (issue #258): an event its day, place and
	// people; a person what they are to the owner, or where they work; a fact the words
	// that matched. A repeating event is one result, unfolding to its dates.
	import type { FoundView } from './api.ts';
	import { dayOf, hrefOf, isDay, longDay, monthYear, periodName, shortDay, timeOf } from './format.ts';
	import { i18n } from './i18n.svelte.ts';
	import { iconOf } from './kinds.ts';

	let { item, compact = false }: { item: FoundView; compact?: boolean } = $props();

	let open = $state(false);

	/** "12 oct. 2025, 19:00", or the day alone for a whole day. */
	function moment(start: string): string {
		const day = shortDay(dayOf(start));
		return isDay(start) ? day : `${day}, ${timeOf(start)}`;
	}

	const line = $derived.by(() => {
		const parts: string[] = [];
		if (item.series) {
			const s = item.series.series;
			parts.push(i18n.t.every(s.weekday, s.time), i18n.t.seriesTimes(s.occurrences, monthYear(s.since)));
		} else if (item.start) {
			parts.push(moment(item.start));
		} else {
			parts.push(i18n.t.kind[item.kind] ?? item.kind);
		}
		if (item.relation) parts.push(item.relation);
		if (item.organization) parts.push(item.organization.name);
		if (item.place) parts.push(item.place.name);
		// Three people at most: a weekly meeting's whole team would hide the rest.
		const people = item.people.map((p) => p.name);
		if (people.length) parts.push(people.slice(0, 3).join(', ') + (people.length > 3 ? ` +${people.length - 3}` : ''));
		return parts.join(' · ');
	});
</script>

<a href={hrefOf({ id: item.id, kind: item.kind, name: item.name }) ?? '/summaries'}>
	<span class="kind-icon" aria-hidden="true"><svg viewBox="0 0 24 24" width="18" height="18"><path d={iconOf(item.kind)} /></svg></span>
	<span class="who">
		<span class="name">{item.kind === 'summary' ? periodName(item.name) : item.kind === 'note' ? i18n.t.dayNoteOf(longDay(item.name)) : item.name}</span>
		<span class="aliases">{line}</span>
		{#if item.excerpt.length}
			<span class="excerpt" class:one-line={compact}
				>{#if i18n.t.matchedIn[item.matched]}<span class="matched-in">{i18n.t.matchedIn[item.matched]}</span>
				{/if}{#each item.excerpt as part, i (i)}{#if part.hit}<mark>{part.text}</mark>{:else}{part.text}{/if}{/each}</span
			>
		{/if}
	</span>
</a>
{#if item.series && item.series.occurrences.length > 1 && !compact}
	<button type="button" class="unfold" aria-expanded={open} onclick={() => (open = !open)}>
		{open ? i18n.t.hideSeries : i18n.t.showOccurrences(item.series.occurrences.length)}
	</button>
	{#if open}
		<ol class="occurrences">
			{#each item.series.occurrences as o (o.id)}
				<li><a href="/agenda/{o.id}">{moment(o.start)}</a></li>
			{/each}
		</ol>
	{/if}
{/if}
