<script lang="ts">
	// What each AI read, day by day: categories and counts, never what was asked.
	import { api, type AccessesView } from './api.ts';
	import { Loader } from './remote.svelte.ts';
	import { capitalize, longDay } from './format.ts';
	import { i18n } from './i18n.svelte.ts';

	const remote = new Loader<AccessesView>();
	remote.load(() => api.accesses());
	const view = $derived(remote.state.kind === 'ready' ? remote.state.value : null);
	const failed = $derived(remote.state.kind === 'failed');
	const today = new Date().toISOString().slice(0, 10);
	const days = $derived(view ? [...new Set(view.days.map((d) => d.day))] : []);
</script>

<p class="quiet">{i18n.t.accessesLead(30)}</p>
{#if failed}
	<p class="quiet">{i18n.t.failed}</p>
{:else if view && !view.days.length}
	<p class="quiet">{i18n.t.noAccess}</p>
{:else if view}
	<div class="access-days" data-testid="accesses">
		{#each days as day (day)}
			<section aria-labelledby="day-{day}">
				<h3 id="day-{day}">{longDay(day, today)}</h3>
				<ul class="access-list">
					{#each view.days.filter((d) => d.day === day) as access (access.ai)}
						<li>
							<span class="ai-name">{access.ai}</span>
							<span class="quiet">{i18n.t.calls(access.calls)}, {i18n.t.results(access.results)}</span>
							<span class="tags">
								{#each access.categories as category (category.name)}
									<span class="badge">{capitalize(i18n.category(category.name))} <span class="quiet">× {category.calls}</span></span>
								{:else}
									<span class="quiet">{i18n.t.noCategory}</span>
								{/each}
							</span>
						</li>
					{/each}
				</ul>
			</section>
		{/each}
	</div>
{/if}
