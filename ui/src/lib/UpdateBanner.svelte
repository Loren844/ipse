<script lang="ts">
	// A new version of the desktop app (issue #156): said at the top of every screen
	// until the person installs it or says "later". Installing restarts the app; a
	// version that changes the memory's format says so first.
	import { appUpdate } from './app.svelte.ts';
	import { i18n } from './i18n.svelte.ts';
	import Notice from './Notice.svelte';
</script>

{#if appUpdate.value}
	{@const update = appUpdate.value}
	<section class="update-banner" aria-labelledby="update-title" data-testid="update">
		<h2 id="update-title">{i18n.t.updateAvailable(update.version)}</h2>
		{#if update.format_changes}
			<p class="update-format"><strong>{i18n.t.updateFormat}</strong> {i18n.t.updateFormatHint}</p>
		{/if}
		{#if update.notes}
			<details>
				<summary>{i18n.t.updateNotes}</summary>
				<p class="update-notes">{update.notes}</p>
			</details>
		{/if}
		<div class="actions">
			<button type="button" class="button primary" disabled={appUpdate.installing} onclick={() => appUpdate.install()}
				>{appUpdate.installing ? i18n.t.updateInstalling : i18n.t.updateInstall}</button
			>
			<button type="button" class="button" disabled={appUpdate.installing} onclick={() => appUpdate.later()}>{i18n.t.updateLater}</button>
		</div>
		<p class="quiet" role="status">{appUpdate.installing ? i18n.t.updateRestarts : i18n.t.updateSigned}</p>
	</section>
{:else if appUpdate.failed}
	<div class="update-banner failed"><Notice tone="warn">{i18n.t.updateFailed}</Notice></div>
{/if}
