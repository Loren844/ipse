<script lang="ts">
	import FirstVisit from '#lib/FirstVisit.svelte';
	import { tick } from 'svelte';
	import AiRights from '#lib/AiRights.svelte';
	import { api, Conflict, SessionEnded, type AisView, type AiView, type CategoryView, type ConnectedView } from '#lib/api.ts';
	import { app } from '#lib/app.svelte.ts';
	import Confirm, { type Question } from '#lib/Confirm.svelte';
	import Connect from '#lib/Connect.svelte';
	import { capitalize } from '#lib/format.ts';
	import { i18n } from '#lib/i18n.svelte.ts';
	import Notice from '#lib/Notice.svelte';
	import { Loader } from '#lib/remote.svelte.ts';

	const screen = new Loader<AisView>();
	screen.load(() => api.ais());

	let busy = $state(false);
	let outcome: { tone: 'done' | 'warn'; text: string } | null = $state(null);
	let question: Question | null = $state(null);
	let connecting = $state(false);
	let connectButton: HTMLButtonElement | undefined = $state();

	let label = $state('');
	let shared = $state(false);
	let openTo: string[] = $state([]);

	const view = $derived(screen.state.kind === 'ready' ? screen.state.value : null);
	const name = (c: CategoryView) => c.label ?? capitalize(i18n.category(c.name));

	function ask(title: string, body: string, yes: string, no: string): Promise<boolean> {
		return new Promise((resolve) => (question = { title, body, yes, no, resolve }));
	}

	/** Runs a change; the screen then shows what the server says, or reads it again. */
	async function run<T>(action: () => Promise<T>, done: (result: T) => string): Promise<T | null> {
		busy = true;
		let result: T | null = null;
		try {
			result = await action();
			outcome = { tone: 'done', text: done(result) };
		} catch (error) {
			if (error instanceof SessionEnded) app.state = { kind: 'ended' };
			const refused = error instanceof Conflict ? (error.message.includes('left as it is') ? i18n.t.installRefused : i18n.t.refusedChange) : null;
			outcome = { tone: 'warn', text: refused ?? i18n.t.actionFailed };
			await screen.load(() => api.ais(), true);
		}
		busy = false;
		return result;
	}

	const show = (next: AisView) => (screen.state = { kind: 'ready', value: next });

	async function setRights(ai: AiView, read: string[], propose: string[]) {
		const next = await run(() => api.setRights(ai.id, read, propose), () => i18n.t.saved(ai.name));
		if (next) show(next);
	}

	async function mayOpen(ai: AiView, category: CategoryView): Promise<boolean> {
		if (!category.sensitive || ai.read.includes(category.name)) return true;
		return ask(i18n.t.openTitle(name(category), ai.name), i18n.t.openBody(ai.name), i18n.t.openConfirm(ai.name), i18n.t.keepClosed);
	}

	async function read(ai: AiView, category: CategoryView, on: boolean) {
		if (on && !(await mayOpen(ai, category))) return;
		const read = on ? [...ai.read, category.name] : ai.read.filter((c) => c !== category.name);
		// An AI proposes only where it reads.
		const propose = on ? ai.propose : ai.propose.filter((c) => c !== category.name);
		await setRights(ai, read, propose);
	}

	async function propose(ai: AiView, category: CategoryView, on: boolean) {
		if (on && !(await mayOpen(ai, category))) return;
		const read = on && !ai.read.includes(category.name) ? [...ai.read, category.name] : ai.read;
		const propose = on ? [...ai.propose, category.name] : ai.propose.filter((c) => c !== category.name);
		await setRights(ai, read, propose);
	}

	async function auto(ai: AiView, standing: string, on: boolean) {
		const what = i18n.t.standingsOf[standing];
		const next = await run(
			() => api.autoAccept(ai.id, standing, on),
			() => (on ? i18n.t.autoOn(ai.name, what) : i18n.t.autoOff(ai.name, what))
		);
		if (next) show(next);
	}

	async function disconnect(ai: AiView) {
		const yes = await ask(i18n.t.disconnectTitle(ai.name), i18n.t.disconnectBody, i18n.t.disconnect, i18n.t.cancelForm);
		if (!yes) return;
		const next = await run(() => api.disconnect(ai.id), () => i18n.t.disconnected(ai.name));
		if (next) show(next);
	}

	async function connect(name: string, local: boolean, openAll: boolean, install?: string): Promise<ConnectedView | null> {
		const connected = await run(() => api.connect(name, local, openAll, install), () => '');
		outcome = connected ? null : outcome;
		if (connected) await screen.load(() => api.ais(), true);
		return connected;
	}

	async function closeConnect() {
		connecting = false;
		await tick();
		connectButton?.focus();
	}

	async function addCategory(e: SubmitEvent) {
		e.preventDefault();
		const added = label.trim();
		const next = await run(() => api.addCategory(added, !shared, shared ? openTo : []), () => i18n.t.categoryAdded(added));
		if (next) {
			show(next);
			label = '';
			shared = false;
			openTo = [];
		}
	}

	function readers(view: AisView, category: CategoryView): string {
		const names = view.ais.filter((a) => a.read.includes(category.name)).map((a) => a.name);
		if (!names.length) return i18n.t.readByNone;
		const list = new Intl.ListFormat(i18n.language === 'fr' ? 'fr-FR' : 'en-GB', { type: 'conjunction' }).format(names);
		return i18n.t.readBy(list);
	}
</script>

<svelte:head><title>{i18n.t.ais} · Ipse</title></svelte:head>

{#if view}
	<div class="screen">
		<header class="page-head">
			<h1>{i18n.t.ais}</h1>
			<p class="intro">{i18n.t.aisLead}</p>
			{#if !connecting}
				<div class="actions">
					<button type="button" class="button primary" bind:this={connectButton} onclick={() => (connecting = true)}>{i18n.t.connectAi}</button>
				</div>
			{/if}
		</header>
		<FirstVisit screen="ais" />

		<div class="live" role="status" aria-live="polite">
			{#if outcome}<Notice tone={outcome.tone}>{outcome.text}</Notice>{/if}
		</div>

		{#if connecting}
			<Connect {busy} known={view.apps} onconnect={connect} onclose={closeConnect} />
		{/if}

		{#if view.ais.length}
			{#each view.ais as ai (ai.id)}
				<AiRights
					{ai}
					categories={view.categories}
					{busy}
					onread={(category, on) => read(ai, category, on)}
					onpropose={(category, on) => propose(ai, category, on)}
					onauto={(standing, on) => auto(ai, standing, on)}
					ondisconnect={() => disconnect(ai)}
				/>
			{/each}
		{:else if !connecting}
			<div class="empty-thread">
				<p class="lead">{i18n.t.aisEmpty}</p>
				<p>{i18n.t.aisEmptyHint}</p>
			</div>
		{/if}

		<section aria-labelledby="categories">
			<h2 id="categories">{i18n.t.categoriesTitle}</h2>
			<p class="quiet">{i18n.t.categoriesLead}</p>
			<ul class="categories" data-testid="categories">
				{#each view.categories as category (category.name)}
					<li>
						<span class="category-name">{name(category)}</span>
						<span class="tags">
							{#if category.custom}<span class="badge">{i18n.t.yourCategory}</span>{/if}
							{#if category.sensitive}<span class="lock">{category.implicit ? i18n.t.sensitiveImplicit : i18n.t.sensitiveTag}</span>{/if}
						</span>
						<span class="readers">{readers(view, category)}</span>
					</li>
				{/each}
			</ul>

			<form class="form add-category" onsubmit={addCategory}>
				<h3>{i18n.t.addCategory}</h3>
				<div class="form-field">
					<label for="category-name">{i18n.t.categoryName}</label>
					<input id="category-name" type="text" bind:value={label} required maxlength="80" aria-describedby="category-name-hint" autocomplete="off" />
					<p class="hint" id="category-name-hint">{i18n.t.categoryNameHint}</p>
				</div>
				<fieldset>
					<legend>{i18n.t.mayAisSee}</legend>
					<div class="choices column">
						<label class="choice"><input type="radio" name="shared" checked={!shared} onchange={() => (shared = false)} />{i18n.t.noSensitive}</label>
						<label class="choice"><input type="radio" name="shared" checked={shared} onchange={() => (shared = true)} />{i18n.t.yesShared}</label>
					</div>
				</fieldset>
				{#if shared && view.ais.length}
					<fieldset>
						<legend>{i18n.t.openTo}</legend>
						<div class="choices">
							{#each view.ais as ai (ai.id)}
								<label class="choice"><input type="checkbox" bind:group={openTo} value={ai.id} />{ai.name}</label>
							{/each}
						</div>
						<p class="hint">{i18n.t.openToHint}</p>
					</fieldset>
				{/if}
				<div class="actions">
					<button type="submit" class="button" disabled={busy || !label.trim()}>{i18n.t.addCategory}</button>
				</div>
			</form>
		</section>
	</div>
	<Confirm bind:question />
{:else if screen.state.kind === 'failed'}
	<p class="quiet">{i18n.t.failed}</p>
{/if}
