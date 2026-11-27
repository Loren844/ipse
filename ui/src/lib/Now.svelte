<script lang="ts">
	// What is going on right now, by topic: the music, the series, the book of the moment.
	// States of one topic last seen the same day share that day, said once: the state
	// log keeps days, not hours, so "Home" and "Work" on one day are not put in an order
	// it does not know (issue #260).
	import type { StateView } from './api.ts';
	import { longDay } from './format.ts';
	import { i18n } from './i18n.svelte.ts';
	import Mention from './Mention.svelte';

	let { states, today }: { states: StateView[]; today: string } = $props();

	/** Each topic, in order of first appearance, with its states grouped by day. */
	const topics = $derived.by(() => {
		const found = new Map<string, { day: string; states: StateView[] }[]>();
		for (const state of states) {
			const day = state.last_seen.slice(0, 10);
			const days = found.get(state.topic) ?? [];
			const same = days.find((d) => d.day === day);
			if (same) same.states.push(state);
			else days.push({ day, states: [state] });
			found.set(state.topic, days);
		}
		return [...found];
	});
</script>

<dl class="now" data-testid="now">
	{#each topics as [topic, days] (topic)}
		<div>
			<dt>{i18n.t.topics[topic] ?? topic}</dt>
			<dd>
				{#each days as { day, states: seen } (day)}
					<span class="state">
						{#each seen as state, i (i)}<Mention link={state.subject} />{/each}
						<span class="when">{longDay(day, today)}</span>
					</span>
				{/each}
			</dd>
		</div>
	{/each}
</dl>
