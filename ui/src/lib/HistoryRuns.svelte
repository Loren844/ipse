<script lang="ts">
	// Each import and sync: what came in, what did not work, and the connector's
	// diagnostic lines on demand.
	import { api, type RunView, type RunsView } from './api.ts';
	import { Loader } from './remote.svelte.ts';
	import { ago, capitalize, sources } from './format.ts';
	import { i18n } from './i18n.svelte.ts';
	import Notice from './Notice.svelte';

	const remote = new Loader<RunsView>();
	remote.load(() => api.runs());
	const view = $derived(remote.state.kind === 'ready' ? remote.state.value : null);
	const failed = $derived(remote.state.kind === 'failed');
	let open: Record<string, string[] | null> = $state({});

	function counts(run: RunView): string {
		const t = i18n.t;
		const parts = [
			run.added && t.countAdded(run.added),
			run.updated && t.updated(run.updated),
			run.unchanged && t.unchanged(run.unchanged),
			run.cancelled && t.cancelledEvents(run.cancelled),
			run.removed && t.removedItems(run.removed),
			run.skipped && t.skipped(run.skipped),
			run.rejected && t.rejected(run.rejected),
			run.tasks && t.tasksMade(run.tasks)
		].filter(Boolean) as string[];
		return parts.length ? capitalize(parts.join(', ')) + '.' : t.nothingNew;
	}

	async function toggle(instance: string) {
		if (open[instance] !== undefined) {
			delete open[instance];
			return;
		}
		open[instance] = null;
		try {
			open[instance] = (await api.diagnostics(instance)).lines;
		} catch {
			delete open[instance];
		}
	}

	/** The diagnostics are offered on the most recent run of each instance. */
	function firstOf(run: RunView, index: number): boolean {
		return view!.runs.findIndex((r) => r.instance === run.instance) === index;
	}
</script>

<p class="quiet">{i18n.t.runsLead}</p>
{#if failed}
	<p class="quiet">{i18n.t.failed}</p>
{:else if view && !view.runs.length}
	<p class="quiet">{i18n.t.noRuns}</p>
{:else if view}
	<ol class="runs" data-testid="runs">
		{#each view.runs as run, i (i)}
			<li class="run" class:failed={run.outcome !== 'done'}>
				<p class="run-head">
					<strong>{capitalize(sources([run.instance]))}</strong>
					<span class="quiet">{run.mode === 'sync' ? i18n.t.modeSync : i18n.t.modeImport}, {ago(run.at)}</span>
				</p>
				{#if run.outcome === 'done'}
					<p>{counts(run)}</p>
				{:else}
					<Notice tone="warn">{i18n.t.runProblems[run.code ?? ''] ?? i18n.t.runFailed} {#if run.added || run.updated}{counts(run)}{/if}</Notice>
				{/if}
				{#if run.conflicts}<p><a href="/review">{i18n.t.conflicts(run.conflicts)}</a></p>{/if}
				{#if run.repaired}<Notice tone="warn">{i18n.t.repaired(run.repaired)}</Notice>{/if}
				{#if view.diagnosed.includes(run.instance) && firstOf(run, i)}
					<button type="button" class="link-button" aria-expanded={open[run.instance] !== undefined} onclick={() => toggle(run.instance)}
						>{open[run.instance] !== undefined ? i18n.t.hideDiagnostics : i18n.t.showDiagnostics}</button
					>
					{#if open[run.instance]}
						{@const lines = open[run.instance] ?? []}
						<p class="hint">{i18n.t.diagnosticsHint(lines.length)}</p>
						<pre class="diagnostics" aria-label={i18n.t.showDiagnostics}>{lines.join('\n')}</pre>
					{/if}
				{/if}
			</li>
		{/each}
	</ol>
{/if}
