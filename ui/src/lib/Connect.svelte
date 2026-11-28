<script lang="ts">
	// Connecting an AI: its name, its app, whether it runs on this computer. For an app
	// Ipse knows (issues #105 and #266), Ipse writes the configuration itself, the code
	// included, which is then shown nowhere; otherwise the code is shown once, with what
	// to paste: for that app, or for any other.
	import { tick } from 'svelte';
	import type { AppView, ConnectedView } from './api.ts';
	import Copy from './Copy.svelte';
	import { i18n } from './i18n.svelte.ts';

	type App = string;
	let {
		busy,
		known = [],
		onconnect,
		onclose
	}: {
		busy: boolean;
		known?: AppView[];
		onconnect: (name: string, local: boolean, openAll: boolean, install?: string, app?: string) => Promise<ConnectedView | null>;
		onclose: () => void;
	} = $props();

	let name = $state('Claude');
	let app: App = $state('claude-desktop');
	let local = $state(false);
	let openAll = $state(false);
	let connected: ConnectedView | null = $state(null);
	let heading: HTMLElement | undefined = $state();

	// The AI is named after its app by default; Claude Desktop's is simply Claude.
	const NAMES: Record<string, string> = { 'claude-desktop': 'Claude' };
	const apps: { value: App; label: string; name: string; local: boolean }[] = $derived([
		...(known.length
			? known.map((k) => ({ value: k.id, label: k.label, name: NAMES[k.id] ?? k.label, local: k.local }))
			: [{ value: 'claude-desktop', label: 'Claude Desktop', name: 'Claude', local: false }]),
		{ value: 'other', label: i18n.t.otherApp, name: '', local: false }
	]);
	const target = $derived(known.find((k) => k.id === app) ?? null);
	// Offered when the app is on this computer; the person can still choose.
	let install = $derived(target?.found ?? false);
	const label = $derived(apps.find((a) => a.value === app)?.label ?? '');

	function pick(value: App) {
		const before = apps.find((a) => a.value === app);
		const next = apps.find((a) => a.value === value)!;
		if (!name || name === before?.name) name = next.name;
		local = next.local;
		app = value;
	}

	async function submit(e: SubmitEvent) {
		e.preventDefault();
		connected = await onconnect(name, local, local && openAll, target && install ? app : undefined, app);
		if (connected) {
			await tick();
			heading?.focus();
		}
	}

	const url = 'http://127.0.0.1:7420/mcp';
	const fileName = (path: string) => path.split(/[\\/]/).pop() ?? path;
	const claudeCode = (c: ConnectedView) => `claude mcp add ipse --scope user -e IPSE_TOKEN=${c.code} -- "${c.program}" mcp --vault "${c.vault}"`;
	const serve = (c: ConnectedView) => `"${c.program}" mcp --http --vault "${c.vault}"`;
</script>

<section class="connect" aria-labelledby="connect-title">
	{#if connected}
		<h2 id="connect-title" tabindex="-1" bind:this={heading}>{i18n.t.connectedTitle(connected.ai.name)}</h2>
		{#if connected.installed}
			<p data-testid="installed">{i18n.t.installedIn(label, connected.installed.path)}</p>
			{#if connected.installed.backup}<p class="quiet">{i18n.t.backupKept(connected.installed.backup)}</p>{/if}
			<p>{i18n.t.nextSteps[connected.installed.app]}</p>
			<div class="actions"><button type="button" class="button primary" onclick={onclose}>{i18n.t.gotIt}</button></div>
		{:else if connected.code}
			<p>{i18n.t.codeOnce}</p>
			<Copy text={connected.code} label="IPSE_TOKEN" />
			{#if app === 'claude-code'}
				<p>{i18n.t.claudeCodeSteps}</p>
				<Copy text={claudeCode(connected)} label="claude mcp add" />
			{:else if target && connected.snippet}
				<p data-testid="paste-steps">{i18n.t.pasteSteps[app] ?? i18n.t.pasteIn(target.label, target.file)}</p>
				<Copy text={connected.snippet} label={fileName(target.file)} />
			{:else}
				<p data-testid="paste-steps">{i18n.t.anyAppSteps}</p>
				<Copy text={connected.command} label={i18n.t.anyAppCommand} />
				{#if connected.snippet}
					<p>{i18n.t.anyAppJson}</p>
					<Copy text={connected.snippet} label="mcpServers" />
				{/if}
				<details class="http-way">
					<summary>{i18n.t.anyAppHttp}</summary>
					<p>{i18n.t.httpSteps}</p>
					<Copy text={serve(connected)} label="ipse mcp --http" />
					<p>{i18n.t.otherSteps}</p>
					<Copy text={`${url}\nAuthorization: Bearer ${connected.code}`} label="MCP" />
				</details>
			{/if}
			<div class="actions"><button type="button" class="button primary" onclick={onclose}>{i18n.t.done}</button></div>
		{/if}
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
			{#if target}
				<label class="check">
					<input type="checkbox" bind:checked={install} aria-describedby="install-hint" />
					<span
						>{i18n.t.installFor(target.label)}<span class="hint" id="install-hint"
							>{target.found ? i18n.t.installHint(target.label) : i18n.t.notFoundHere(target.label)}</span
						></span
					>
				</label>
			{/if}
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
