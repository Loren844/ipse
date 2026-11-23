<script lang="ts">
	// One connected AI: what it reads and where it proposes, one switch each per
	// category. The page asks before opening a sensitive category.
	import type { AiView, CategoryView } from './api.ts';
	import { ago, capitalize } from './format.ts';
	import { i18n } from './i18n.svelte.ts';
	import { icons } from './icons.ts';

	let {
		ai,
		categories,
		busy,
		onread,
		onpropose,
		onauto,
		ondisconnect
	}: {
		ai: AiView;
		categories: CategoryView[];
		busy: boolean;
		onread: (category: CategoryView, on: boolean) => void;
		onpropose: (category: CategoryView, on: boolean) => void;
		onauto: (standing: string, on: boolean) => void;
		ondisconnect: () => void;
	} = $props();

	const name = (c: CategoryView) => c.label ?? capitalize(i18n.category(c.name));
	const STANDINGS = ['summaries', 'event_kinds'];
</script>

<section class="ai" aria-labelledby="ai-{ai.id}" data-testid="ai">
	<header class="ai-head">
		<span class="ai-mark" aria-hidden="true"><svg viewBox="0 0 24 24" width="20" height="20"><path d={icons.ais} /></svg></span>
		<div>
			<h2 id="ai-{ai.id}">{ai.name}</h2>
			<p class="meta">
				{#if ai.local}<span class="badge">{i18n.t.onThisComputer}</span>{/if}
				<span>{i18n.t.connectedOn(ago(ai.created))}</span>
				<span>{ai.last_access ? i18n.t.lastAccess(ago(ai.last_access)) : i18n.t.neverUsed}</span>
			</p>
		</div>
	</header>

	<table class="rights">
		<thead>
			<tr>
				<th scope="col">{i18n.t.category}</th>
				<th scope="col" class="switch-col">{i18n.t.canRead}</th>
				<th scope="col" class="switch-col">{i18n.t.canPropose}</th>
			</tr>
		</thead>
		<tbody>
			{#each categories as category (category.name)}
				{@const reads = ai.read.includes(category.name)}
				{@const proposes = ai.propose.includes(category.name)}
				<tr class:sensitive-row={category.sensitive}>
					<th scope="row">
						<span class="category-name">{name(category)}</span>
						{#if category.sensitive}
							<span class="lock" class:open={reads}>
								<svg viewBox="0 0 24 24" width="14" height="14" aria-hidden="true"
									><rect x="5" y="10.5" width="14" height="10" rx="2.5" /><path d={reads ? 'M8.5 10.5V7.5a3.5 3.5 0 0 1 6.8-1.2' : 'M8.5 10.5V7.5a3.5 3.5 0 0 1 7 0v3'} /></svg
								>
								{category.implicit ? i18n.t.sensitiveImplicit : i18n.t.sensitiveTag}
							</span>
						{/if}
					</th>
					<td class="switch-col">
						<input
							type="checkbox"
							role="switch"
							class="switch"
							checked={reads}
							disabled={busy}
							aria-label={i18n.t.readSwitch(ai.name, name(category))}
							onchange={(e) => {
								const on = e.currentTarget.checked;
								e.currentTarget.checked = reads;
								onread(category, on);
							}}
						/>
					</td>
					<td class="switch-col">
						<input
							type="checkbox"
							role="switch"
							class="switch"
							checked={proposes}
							disabled={busy}
							aria-label={i18n.t.proposeSwitch(ai.name, name(category))}
							onchange={(e) => {
								const on = e.currentTarget.checked;
								e.currentTarget.checked = proposes;
								onpropose(category, on);
							}}
						/>
					</td>
				</tr>
			{/each}
		</tbody>
	</table>

	<div class="auto-accept" role="group" aria-labelledby="auto-{ai.id}">
		<h3 id="auto-{ai.id}">{i18n.t.autoTitle}</h3>
		<p class="hint">{i18n.t.autoLead(ai.name)}</p>
		<ul>
			{#each STANDINGS as standing (standing)}
				{@const on = ai.auto_accept.includes(standing)}
				<li>
					<label>
						<input
							type="checkbox"
							role="switch"
							class="switch"
							checked={on}
							disabled={busy}
							aria-label={i18n.t.autoSwitch(ai.name, i18n.t.standingsOf[standing])}
							onchange={(e) => {
								const next = e.currentTarget.checked;
								e.currentTarget.checked = on;
								onauto(standing, next);
							}}
						/>
						<span>{i18n.t.standings[standing]}</span>
					</label>
				</li>
			{/each}
		</ul>
	</div>

	<div class="actions">
		<button type="button" class="button quiet-button" disabled={busy} onclick={ondisconnect}>{i18n.t.disconnect}</button>
	</div>
</section>
