<script lang="ts">
	// Connecting an AI: its name, its app, whether it runs on this computer; then its
	// access code, once, with what to paste in that app.
	import { tick } from 'svelte';
	import type { ConnectedView } from './api.ts';
	import Copy from './Copy.svelte';
	import { i18n } from './i18n.svelte.ts';

	type App = 'claude' | 'lmstudio' | 'other';
	let {
		busy,
		onconnect,
		onclose
	}: {
		busy: boolean;
		onconnect: (name: string, local: boolean, openAll: boolean) => Promise<ConnectedView | null>;
		onclose: () => void;
	} = $props();

	let name = $state('Claude');
	let app: App = $state('claude');
	let local = $state(false);
	let openAll = $state(false);
	let connected: ConnectedView | null = $state(null);
	let heading: HTMLElement | undefined = $state();

	const apps: { value: App; label: string; name: string; local: boolean }[] = $derived([
		{ value: 'claude', label: 'Claude Desktop', name: 'Claude', local: false },
		{ value: 'lmstudio', label: 'LM Studio', name: 'LM Studio', local: true },
		{ value: 'other', label: i18n.t.otherApp, name: '', local: false }
	]);

	function pick(value: App) {
		const before = apps.find((a) => a.value === app);
		const next = apps.find((a) => a.value === value)!;
		if (!name || name === before?.name) name = next.name;
		local = next.local;
		app = value;
	}

	async function submit(e: SubmitEvent) {
		e.preventDefault();
		connected = await onconnect(name, local, local && openAll);
		if (connected) {
			await tick();
			heading?.focus();
		}
	}

	const url = 'http://127.0.0.1:7420/mcp';
	const json = (value: unknown) => JSON.stringify(value, null, 2);
	const claudeConfig = (c: ConnectedView) =>
		json({ mcpServers: { ipse: { command: c.program, args: ['mcp', '--vault', c.vault], env: { IPSE_TOKEN: c.code } } } });
	const httpConfig = (c: ConnectedView) => json({ mcpServers: { ipse: { url, headers: { Authorization: `Bearer ${c.code}` } } } });
	const serve = (c: ConnectedView) => `"${c.program}" mcp --http --vault "${c.vault}"`;
</script>

<section class="connect" aria-labelledby="connect-title">
	{#if connected}
		<h2 id="connect-title" tabindex="-1" bind:this={heading}>{i18n.t.connectedTitle(connected.ai.name)}</h2>
		<p>{i18n.t.codeOnce}</p>
		<Copy text={connected.code} label="IPSE_TOKEN" />
		{#if app === 'claude'}
			<p>{i18n.t.claudeSteps}</p>
			<Copy text={claudeConfig(connected)} label="claude_desktop_config.json" />
		{:else}
			<p>{i18n.t.httpSteps}</p>
			<Copy text={serve(connected)} label="ipse mcp --http" />
			{#if app === 'lmstudio'}
				<p>{i18n.t.lmStudioSteps}</p>
				<Copy text={httpConfig(connected)} label="mcp.json" />
			{:else}
				<p>{i18n.t.otherSteps}</p>
				<Copy text={`${url}\nAuthorization: Bearer ${connected.code}`} label="MCP" />
			{/if}
		{/if}
		<div class="actions"><button type="button" class="button primary" onclick={onclose}>{i18n.t.done}</button></div>
	{:else}
		<h2 id="connect-title">{i18n.t.connectAi}</h2>
		<form class="form" onsubmit={submit}>
			<fieldset>
				<legend>{i18n.t.whichApp}</legend>
				<div class="choices">
					{#each apps as choice (choice.value)}
						<label class="choice">
							<input type="radio" name="app" value={choice.value} checked={app === choice.value} onchange={() => pick(choice.value)} />
							{choice.label}
						</label>
					{/each}
				</div>
			</fieldset>
			<div class="form-field">
				<label for="ai-name">{i18n.t.aiName}</label>
				<input id="ai-name" type="text" bind:value={name} required maxlength="80" aria-describedby="ai-name-hint" autocomplete="off" />
				<p class="hint" id="ai-name-hint">{i18n.t.aiNameHint}</p>
			</div>
			<label class="check">
				<input type="checkbox" bind:checked={local} aria-describedby="local-hint" />
				<span>{i18n.t.isLocal}<span class="hint" id="local-hint">{i18n.t.isLocalHint}</span></span>
			</label>
			{#if local}
				<label class="check">
					<input type="checkbox" bind:checked={openAll} aria-describedby="open-all-hint" />
					<span>{i18n.t.openAll}<span class="hint" id="open-all-hint">{i18n.t.openAllHint}</span></span>
				</label>
			{/if}
			<div class="actions">
				<button type="submit" class="button primary" disabled={busy || !name.trim()}>{i18n.t.connect}</button>
				<button type="button" class="button" onclick={onclose}>{i18n.t.cancelForm}</button>
			</div>
		</form>
	{/if}
</section>
