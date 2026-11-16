<script lang="ts">
	// What is going on right now, by topic: the music, the series, the book of the moment.
	import type { StateView } from './api.ts';
	import { longDay } from './format.ts';
	import { i18n } from './i18n.svelte.ts';
	import Mention from './Mention.svelte';

	let { states, today }: { states: StateView[]; today: string } = $props();

	const topics = $derived([...new Set(states.map((s) => s.topic))]);
</script>

<dl class="now" data-testid="now">
	{#each topics as topic (topic)}
		<div>
			<dt>{i18n.t.topics[topic] ?? topic}</dt>
			<dd>
				{#each states.filter((s) => s.topic === topic) as state, i (i)}
					<span class="state">
						<Mention link={state.subject} />
						<span class="when">{longDay(state.last_seen.slice(0, 10), today)}</span>
					</span>
				{/each}
			</dd>
		</div>
	{/each}
</dl>
