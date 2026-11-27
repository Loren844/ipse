<script lang="ts">
	import DiskState from '#lib/DiskState.svelte';
	import { api, Conflict, SessionEnded, type SettingsView } from '#lib/api.ts';
	import { app, appLinks, appSettings, inApp } from '#lib/app.svelte.ts';
	import Copy from '#lib/Copy.svelte';
	import { i18n, type Language } from '#lib/i18n.svelte.ts';
	import Notice from '#lib/Notice.svelte';
	import { Loader } from '#lib/remote.svelte.ts';
	import { theme, type ThemeChoice } from '#lib/theme.svelte.ts';
	import { fold, systemZone, zoneList } from '#lib/zones.ts';

	const settings = new Loader<SettingsView>();
	settings.load(() => api.settings());

	const themes: ThemeChoice[] = ['system', 'light', 'dark'];
	const languages: { code: Language; name: string }[] = [
		{ code: 'fr', name: 'Français' },
		{ code: 'en', name: 'English' }
	];
	const system = systemZone();

	let language = $state('');
	let timeZone = $state('');
	let findZone = $state('');
	let busy = $state(false);
	let outcome: { tone: 'done' | 'warn'; text: string } | null = $state(null);

	const ready = $derived(settings.state.kind === 'ready' ? settings.state.value : null);
	const changed = $derived(!!ready && (language !== ready.language || timeZone !== ready.time_zone));
	const locale = $derived(i18n.language === 'fr' ? 'fr-FR' : 'en-GB');
	/** This computer's zone first, then the memory's, in the interface's language. */
	const zones = $derived(ready ? zoneList(locale, system, ready.time_zone) : []);
	/** Those matching the search; the zone chosen always stays, so the list shows it. */
	const shownZones = $derived.by(() => {
		const words = fold(findZone.trim());
		return words ? zones.filter((z) => z.id === timeZone || z.search.includes(words)) : zones;
	});

	$effect(() => {
		if (ready && !language) {
			language = ready.language;
			timeZone = ready.time_zone;
		}
	});

	async function save(e: SubmitEvent) {
		e.preventDefault();
		busy = true;
		// The interface keeps its language: only the words written in files change.
		i18n.choose(i18n.language);
		try {
			const saved = await api.changeSettings(language, timeZone);
			await settings.load(async () => saved, true);
			outcome = { tone: 'done', text: i18n.t.settingsSaved };
			await app.refresh();
		} catch (error) {
			if (error instanceof SessionEnded) app.state = { kind: 'ended' };
			outcome = { tone: 'warn', text: error instanceof Conflict ? i18n.t.settingsRefused : i18n.t.actionFailed };
		}
		busy = false;
	}
</script>

<svelte:head><title>{i18n.t.settings} · Ipse</title></svelte:head>

<div class="screen settings">
	<header class="page-head">
		<h1>{i18n.t.settings}</h1>
	</header>

	<section aria-labelledby="look">
		<h2 id="look">{inApp ? i18n.t.thisApp : i18n.t.thisBrowser}</h2>
		<p class="quiet">{inApp ? i18n.t.thisAppHint : i18n.t.thisBrowserHint}</p>
		<div class="setting">
			<span class="label" id="theme-label">{i18n.t.theme}</span>
			<div class="segmented" role="group" aria-labelledby="theme-label">
				{#each themes as choice (choice)}
					<button type="button" aria-pressed={theme.choice === choice} onclick={() => theme.choose(choice)}
						>{i18n.t.themes[choice]}</button
					>
				{/each}
			</div>
		</div>
		<div class="setting">
			<span class="label" id="language-label">{i18n.t.interfaceLanguage}</span>
			<div class="segmented" role="group" aria-labelledby="language-label">
				{#each languages as choice (choice.code)}
					<button
						type="button"
						lang={choice.code}
						aria-pressed={i18n.language === choice.code}
						onclick={() => i18n.choose(choice.code)}>{choice.name}</button
					>
				{/each}
			</div>
			<p class="hint setting-hint">{i18n.t.interfaceLanguageHint}</p>
		</div>
	</section>

	{#if inApp}
		<section aria-labelledby="background">
			<h2 id="background">{i18n.t.inTheBackground}</h2>
			<p class="quiet">{i18n.t.inTheBackgroundHint}</p>
			<label class="check">
				<input
					type="checkbox"
					checked={appSettings.value?.notifications ?? false}
					disabled={!appSettings.value}
					aria-describedby="notifications-hint"
					onchange={(e) => appSettings.change('notifications', e.currentTarget.checked)}
				/>
				<span>{i18n.t.notifications}<span class="hint" id="notifications-hint">{i18n.t.notificationsHint}</span></span>
			</label>
			<label class="check">
				<input
					type="checkbox"
					checked={appSettings.value?.start ?? false}
					disabled={!appSettings.value}
					aria-describedby="start-hint"
					onchange={(e) => appSettings.change('start', e.currentTarget.checked)}
				/>
				<span>{i18n.t.startWithComputer}<span class="hint" id="start-hint">{i18n.t.startWithComputerHint}</span></span>
			</label>
			{#if appSettings.value && appSettings.value.updates !== null && appSettings.value.updates !== undefined}
				<label class="check">
					<input
						type="checkbox"
						checked={appSettings.value.updates}
						aria-describedby="updates-hint"
						onchange={(e) => appSettings.change('updates', e.currentTarget.checked)}
					/>
					<span>{i18n.t.checkUpdates}<span class="hint" id="updates-hint">{i18n.t.checkUpdatesHint}</span></span>
				</label>
			{/if}
		</section>
	{/if}

	{#if ready}
		<section aria-labelledby="memory">
			<h2 id="memory">{i18n.t.yourMemory}</h2>
			<form class="setting-form" onsubmit={save}>
				<div class="form-field">
					<label for="memory-language">{i18n.t.memoryLanguage}</label>
					<select id="memory-language" bind:value={language} aria-describedby="memory-language-hint">
						{#each languages as choice (choice.code)}<option value={choice.code}>{choice.name}</option>{/each}
					</select>
					<p class="hint" id="memory-language-hint">{i18n.t.memoryLanguageHint}</p>
				</div>
				<div class="form-field">
					<label for="time-zone">{i18n.t.timeZone}</label>
					<input type="search" class="zone-search" bind:value={findZone} placeholder={i18n.t.findZone} aria-label={i18n.t.findZone} aria-controls="time-zone" />
					<select id="time-zone" bind:value={timeZone} aria-describedby="time-zone-hint">
						{#each shownZones as zone (zone.id)}<option value={zone.id}
								>{zone.id === system ? i18n.t.thisComputer(zone.label) : zone.label}</option
							>{/each}
					</select>
					<p class="hint" id="time-zone-hint">{i18n.t.timeZoneHint}</p>
				</div>
				<div class="actions">
					<button type="submit" class="button primary" disabled={busy || !changed}>{i18n.t.saveSettings}</button>
				</div>
			</form>
			<div class="live" role="status" aria-live="polite">
				{#if outcome}<Notice tone={outcome.tone}>{outcome.text}</Notice>{/if}
			</div>

			<div class="setting">
				<span class="label">{i18n.t.memoryFolder}</span>
				<Copy text={ready.folder} label={i18n.t.memoryFolder} />
			</div>
			{#if inApp}
				<div class="setting">
					<span class="label" id="other-memory">{i18n.t.otherMemory}</span>
					<a class="button" href={appLinks.switchMemory} data-sveltekit-reload aria-describedby="other-memory">{i18n.t.switchMemory}</a>
				</div>
			{/if}
			<div class="setting disk" data-testid="disk" data-state={ready.disk.state}>
				<span class="label">{i18n.t.diskTitle}</span>
				<div class="disk-state">
					<span class="badge" class:warn={ready.disk.state === 'off'}>{i18n.t.diskState[ready.disk.state]}</span>
					<DiskState disk={ready.disk} />
				</div>
			</div>
			<div class="setting">
				<span class="label">{i18n.t.version}</span>
				<span data-testid="version">Ipse {ready.version}</span>
			</div>
		</section>
	{:else if settings.state.kind === 'failed'}
		<p class="quiet">{i18n.t.failed}</p>
	{/if}
</div>
