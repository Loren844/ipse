<script lang="ts">
	// What feeds the memory (issue #152): files imported, calendars followed by link,
	// calendars and contacts accounts (CalDAV, CardDAV: issue #154), servers followed for
	// one of their people: Home Assistant (issue #225), Jellyfin (issue #227), Immich (issue
	// #226). Connected sources first, then a catalogue of cards by category; a card opens
	// its form alone, in place of the catalogue, at `/sources?add=<card>` (issue #245).
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import FirstVisit from '#lib/FirstVisit.svelte';
	import { api, SessionEnded, type SourcesChange, type SourcesView, type SourceView } from '#lib/api.ts';
	import { app } from '#lib/app.svelte.ts';
	import Confirm, { type Question } from '#lib/Confirm.svelte';
	import { ago, runCounts } from '#lib/format.ts';
	import { i18n } from '#lib/i18n.svelte.ts';
	import { icons } from '#lib/icons.ts';
	import Notice from '#lib/Notice.svelte';
	import { Loader } from '#lib/remote.svelte.ts';

	const screen = new Loader<SourcesView>();
	screen.load(() => api.sources());
	const view = $derived(screen.state.kind === 'ready' ? screen.state.value : null);

	let outcome: { tone: 'done' | 'warn'; text: string } | null = $state(null);
	let question: Question | null = $state(null);
	/** What is under way: an import (its file's name), following, or a source syncing. */
	let working:
		| { kind: 'import'; name: string }
		| { kind: 'follow' }
		| { kind: 'connect' }
		| { kind: 'connect-server'; connector: Server }
		| { kind: 'sync'; instance: string }
		| { kind: 'choose'; instance: string }
		| { kind: 'positions'; instance: string }
		| null = $state(null);
	let over = $state(false);
	let picker: HTMLInputElement | undefined = $state();
	let calendarName = $state('');
	let link = $state('');
	let accountName = $state('');
	let address = $state('');
	let user = $state('');
	let password = $state('');
	type Server = 'homeassistant' | 'jellyfin' | 'immich';
	const SERVERS: Server[] = ['homeassistant', 'jellyfin', 'immich'];
	/** What is typed in each server's form. */
	let forms: Record<Server, { name: string; address: string; token: string }> = $state({
		homeassistant: { name: '', address: '', token: '' },
		jellyfin: { name: '', address: '', token: '' },
		immich: { name: '', address: '', token: '' }
	});
	/** What can be added, one card each, under the heading of its category. */
	type Card = 'file' | 'link' | 'account' | Server;
	const CATEGORIES: { id: 'calendars' | 'home' | 'media'; cards: Card[] }[] = [
		{ id: 'calendars', cards: ['file', 'link', 'account'] },
		{ id: 'home', cards: ['homeassistant'] },
		{ id: 'media', cards: ['immich', 'jellyfin'] }
	];
	const CARDS: Card[] = CATEGORIES.flatMap((c) => c.cards);
	const ICONS: Record<Card, string> = {
		file: icons.sources,
		link: icons.link,
		account: icons.people,
		homeassistant: icons.home,
		immich: icons.photo,
		jellyfin: icons.play
	};
	/** The card whose form is open, from the address; none shows the catalogue. */
	const adding: Card | null = $derived.by(() => {
		const card = page.url.searchParams.get('add') as Card | null;
		return card && CARDS.includes(card) ? card : null;
	});
	/** Whether one of the sources came from this card: a file can be imported again and again. */
	const connected = (card: Card) =>
		card !== 'file' &&
		(view?.sources ?? []).some((s) =>
			card === 'link' ? s.connector === 'calendar' && s.followed : card === 'account' ? isAccount(s) : s.connector === card
		);
	/** Back to the catalogue once a source is added: it is now at the top, under the notice. */
	const added = () => goto('/sources');
	/** Whom is picked on each server, before it is sent. */
	let persons: Record<string, string> = $state({});
	/** The collections ticked on each account, before they are sent. */
	let picks: Record<string, string[]> = $state({});

	const isAccount = (source: SourceView) => source.connector === 'dav';
	const serverOf = (source: SourceView): Server | null =>
		SERVERS.includes(source.connector as Server) ? (source.connector as Server) : null;
	const connecting = () => (working?.kind === 'connect-server' ? working.connector : null);
	const picked = (source: SourceView) =>
		picks[source.instance] ?? source.account?.collections.filter((c) => c.followed).map((c) => c.url) ?? [];

	function tick(source: SourceView, url: string, on: boolean) {
		const now = picked(source).filter((u) => u !== url);
		picks[source.instance] = on ? [...now, url] : now;
	}

	/** Why a source failed, in the words that fit it. */
	function problem(source: SourceView, code: string | null): string {
		const t = i18n.t;
		const server = serverOf(source);
		const own = isAccount(source) ? t.accountProblems : server ? t.servers[server].problems : {};
		return own[code ?? ''] ?? t.runProblems[code ?? ''] ?? t.runFailed;
	}

	const title = (source: SourceView) => source.name ?? i18n.t.sourceKinds[source.connector] ?? source.connector;
	const nameOf = (instance: string | null, sources: SourcesView) =>
		sources.sources.find((s) => s.instance === instance)?.name ?? '';

	/** The words for what a change did. */
	function said(change: SourcesChange): { tone: 'done' | 'warn'; text: string } {
		const t = i18n.t;
		const name = nameOf(change.source, change.sources);
		switch (change.outcome) {
			case 'followed':
				return { tone: 'done', text: t.followedNow(name) };
			case 'kept':
				return { tone: 'warn', text: t.keptNow(name) };
			case 'unfollowed':
				return { tone: 'done', text: t.unfollowedNow(change.detail ?? '') };
			case 'already':
				return {
					tone: 'warn',
					text:
						working?.kind === 'connect' || connecting()
							? t.alreadyConnected(change.detail ?? '')
							: t.alreadyFollowed(change.detail ?? '')
				};
			case 'connected':
				return { tone: 'done', text: connecting() ? t.serverConnectedNow(name) : t.connectedNow(name) };
			case 'bad_address': {
				const server = connecting();
				return { tone: 'warn', text: server ? t.servers[server].badAddress : t.sourceOutcomes.bad_address };
			}
			case 'not_connected': {
				const server = connecting();
				const why = (server ? t.servers[server].problems : t.accountProblems)[change.detail ?? ''];
				return { tone: 'warn', text: why ?? t.sourceOutcomes.not_connected };
			}
			case 'imported':
			case 'synced':
			case 'positions':
			case 'noPositions':
				return { tone: 'done', text: t.sourceOutcomes[change.outcome] };
			case 'failed': {
				const source = change.sources.sources.find((s) => s.instance === change.source);
				const why = source && (isAccount(source) || serverOf(source)) ? problem(source, change.detail) : t.runProblems[change.detail ?? ''];
				return { tone: 'warn', text: why ? `${t.sourceOutcomes.failed} ${why}` : t.sourceOutcomes.failed };
			}
			default:
				return { tone: 'warn', text: t.sourceOutcomes[change.outcome] ?? t.actionFailed };
		}
	}

	async function run(work: NonNullable<typeof working>, action: () => Promise<SourcesChange>): Promise<SourcesChange | null> {
		working = work;
		outcome = null;
		let change: SourcesChange | null = null;
		try {
			change = await action();
			screen.state = { kind: 'ready', value: change.sources };
			outcome = said(change);
			await app.refresh();
		} catch (error) {
			if (error instanceof SessionEnded) app.state = { kind: 'ended' };
			outcome = { tone: 'warn', text: i18n.t.actionFailed };
		}
		working = null;
		return change;
	}

	function importFiles(files: FileList | null | undefined) {
		const file = files?.[0];
		if (!file || working) return;
		run({ kind: 'import', name: file.name }, () => api.importFile(file)).then((change) => {
			if (change?.outcome === 'imported') added();
		});
		if (picker) picker.value = '';
	}

	function drop(event: DragEvent) {
		event.preventDefault();
		over = false;
		importFiles(event.dataTransfer?.files);
	}

	async function follow(event: SubmitEvent) {
		event.preventDefault();
		const change = await run({ kind: 'follow' }, () => api.followCalendar(link.trim(), calendarName.trim()));
		// The link leaves the page once it is kept.
		if (change && (change.outcome === 'followed' || change.outcome === 'kept')) {
			link = '';
			calendarName = '';
			added();
		}
	}

	async function connect(event: SubmitEvent) {
		event.preventDefault();
		const change = await run({ kind: 'connect' }, () =>
			api.connectAccount(address.trim(), user.trim(), password, accountName.trim())
		);
		// The password leaves the page once it is kept.
		if (change?.outcome === 'connected') {
			password = '';
			address = '';
			user = '';
			accountName = '';
			added();
		}
	}

	async function connectServer(event: SubmitEvent, server: Server) {
		event.preventDefault();
		const form = forms[server];
		const change = await run({ kind: 'connect-server', connector: server }, () =>
			api.connectServer(server, form.address.trim(), form.token, form.name.trim())
		);
		// The token leaves the page once it is kept.
		if (change?.source) {
			forms[server] = { name: '', address: '', token: '' };
			added();
		}
	}

	async function choosePerson(event: SubmitEvent, source: SourceView) {
		event.preventDefault();
		const person = persons[source.instance];
		if (!person) return;
		const change = await run({ kind: 'choose', instance: source.instance }, () => api.chooseOne(source.instance, person));
		if (change) delete persons[source.instance];
	}

	async function setPositions(source: SourceView, on: boolean) {
		await run({ kind: 'positions', instance: source.instance }, () => api.setPositions(source.instance, on));
	}

	async function choose(event: SubmitEvent, source: SourceView) {
		event.preventDefault();
		const change = await run({ kind: 'choose', instance: source.instance }, () =>
			api.chooseCollections(source.instance, picked(source))
		);
		if (change) delete picks[source.instance];
	}

	function ask(source: SourceView): Promise<boolean> {
		const t = i18n.t;
		const server = serverOf(source);
		const body = isAccount(source) ? t.unfollowAccountBody : server ? t.servers[server].unfollowBody : t.unfollowBody;
		return new Promise(
			(resolve) => (question = { title: t.unfollowTitle(title(source)), body, yes: t.unfollow, no: t.keepFollowing, resolve })
		);
	}

	async function unfollow(source: SourceView) {
		if (await ask(source)) run({ kind: 'sync', instance: source.instance }, () => api.unfollow(source.instance));
	}

	/** "Importé 2 fois, dernier import il y a 3 minutes." */
	function status(source: SourceView): string {
		const t = i18n.t;
		const when = source.last ? ago(source.last.at) : null;
		if (source.followed) {
			const every = source.every_minutes ? t.followedEvery(source.every_minutes) : '';
			return `${every}, ${when ? t.lastSync(when) : t.neverSynced}.`;
		}
		return `${t.importedFiles(source.runs)}${when ? `, ${t.lastImport(when)}` : ''}.`;
	}
</script>

<svelte:head><title>{i18n.t.sourcesNav} · Ipse</title></svelte:head>

<div class="screen sources">
	<header class="page-head">
		<h1>{i18n.t.sourcesNav}</h1>
		<p class="intro">{i18n.t.sourcesLead}</p>
	</header>
	<FirstVisit screen="sources" />

	<div class="live" role="status" aria-live="polite">
		{#if working?.kind === 'import'}
			<p class="quiet">{i18n.t.importing(working.name)}</p>
		{:else if working?.kind === 'connect'}
			<p class="quiet">{i18n.t.connecting}</p>
		{:else if working?.kind === 'connect-server'}
			<p class="quiet">{i18n.t.servers[working.connector].connecting}</p>
		{:else if outcome}
			<Notice tone={outcome.tone}>{outcome.text}</Notice>
		{/if}
	</div>

	{#if adding}
		<p class="quiet-link back"><a href="/sources" onclick={() => (outcome = null)}>← {i18n.t.backToSources}</a></p>
		<div class="add">
			{#if adding === 'file'}
			<section aria-labelledby="import">
				<h2 id="import">{i18n.t.importTitle}</h2>
				<p class="quiet">{i18n.t.importHint}</p>
				<div
					class="drop"
					class:over
					role="group"
					aria-labelledby="import"
					ondragover={(e) => {
						e.preventDefault();
						over = true;
					}}
					ondragleave={() => (over = false)}
					ondrop={drop}
				>
					<p>
						{i18n.t.dropHere}
						<label class="link-button" for="file">{i18n.t.chooseFile}</label>
					</p>
					<input
						id="file"
						class="visually-hidden"
						type="file"
						accept=".vcf,.vcard,.ics,.ical,.json"
						bind:this={picker}
						disabled={working !== null}
						onchange={(e) => importFiles(e.currentTarget.files)}
					/>
				</div>
				<details class="exports">
					<summary>{i18n.t.howToExport}</summary>
					{#each i18n.t.exportSteps as step (step.title)}
						<h3>{step.title}</h3>
						<ul>
							{#each step.lines as line (line)}<li>{line}</li>{/each}
						</ul>
					{/each}
				</details>
			</section>
			{:else if adding === 'link'}
			<section aria-labelledby="follow">
				<h2 id="follow">{i18n.t.followTitle}</h2>
				<p class="quiet" id="follow-hint">{i18n.t.followHint}</p>
				<form class="setting-form" onsubmit={follow}>
					<div class="form-field">
						<label for="calendar-name">{i18n.t.calendarName}</label>
						<input id="calendar-name" type="text" bind:value={calendarName} autocomplete="off" />
					</div>
					<div class="form-field">
						<label for="calendar-link">{i18n.t.calendarLink}</label>
						<input
							id="calendar-link"
							type="url"
							bind:value={link}
							required
							autocomplete="off"
							spellcheck="false"
							aria-describedby="follow-hint"
						/>
					</div>
					<div class="actions">
						<button type="submit" class="button primary" disabled={working !== null || !link.trim()}>
							{working?.kind === 'follow' ? i18n.t.following : i18n.t.followGo}
						</button>
					</div>
				</form>
			</section>
			{:else if adding === 'account'}
			<section aria-labelledby="account">
				<h2 id="account">{i18n.t.accountTitle}</h2>
				<p class="quiet" id="account-hint">{i18n.t.accountHint}</p>
				<form class="setting-form" onsubmit={connect}>
					<div class="form-field">
						<label for="account-name">{i18n.t.accountName}</label>
						<input id="account-name" type="text" bind:value={accountName} autocomplete="off" />
					</div>
					<div class="form-field">
						<label for="account-address">{i18n.t.accountAddress}</label>
						<input
							id="account-address"
							type="text"
							inputmode="url"
							bind:value={address}
							required
							autocomplete="off"
							spellcheck="false"
							aria-describedby="account-hint"
						/>
					</div>
					<div class="form-field">
						<label for="account-user">{i18n.t.accountUser}</label>
						<input id="account-user" type="text" bind:value={user} required autocomplete="username" spellcheck="false" />
					</div>
					<div class="form-field">
						<label for="account-password">{i18n.t.accountPassword}</label>
						<input id="account-password" type="password" bind:value={password} required autocomplete="off" />
					</div>
					<div class="actions">
						<button
							type="submit"
							class="button primary"
							disabled={working !== null || !address.trim() || !user.trim() || !password}
						>
							{working?.kind === 'connect' ? i18n.t.connecting : i18n.t.connectGo}
						</button>
					</div>
				</form>
				<details class="exports">
					<summary>{i18n.t.appPasswordHelp}</summary>
					{#each i18n.t.appPasswordSteps as step (step.title)}
						<h3>{step.title}</h3>
						<ul>
							{#each step.lines as line (line)}<li>{line}</li>{/each}
						</ul>
					{/each}
				</details>
			</section>
			{:else if adding}
				{@const server = adding}
				{@const t = i18n.t.servers[server]}
				{@const form = forms[server]}
				<section aria-labelledby={`${server}-title`}>
					<h2 id={`${server}-title`}>{t.title}</h2>
					<p class="quiet" id={`${server}-hint`}>{t.hint}</p>
					<form class="setting-form" onsubmit={(e) => connectServer(e, server)}>
						<div class="form-field">
							<label for={`${server}-name`}>{t.name}</label>
							<input id={`${server}-name`} type="text" bind:value={form.name} autocomplete="off" />
						</div>
						<div class="form-field">
							<label for={`${server}-address`}>{t.address}</label>
							<input
								id={`${server}-address`}
								type="text"
								inputmode="url"
								bind:value={form.address}
								required
								autocomplete="off"
								spellcheck="false"
								aria-describedby={`${server}-hint`}
							/>
						</div>
						<div class="form-field">
							<label for={`${server}-token`}>{t.token}</label>
							<input id={`${server}-token`} type="password" bind:value={form.token} required autocomplete="off" />
						</div>
						<div class="actions">
							<button type="submit" class="button primary" disabled={working !== null || !form.address.trim() || !form.token}>
								{connecting() === server ? t.connecting : t.go}
							</button>
						</div>
					</form>
					<details class="exports">
						<summary>{t.help}</summary>
						{#each t.steps as step (step.title)}
							<h3>{step.title}</h3>
							<ul>
								{#each step.lines as line (line)}<li>{line}</li>{/each}
							</ul>
						{/each}
					</details>
				</section>
			{/if}
		</div>
	{:else}
		<section aria-labelledby="yours">
			<h2 id="yours">{i18n.t.yourSources}</h2>
			{#if view && !view.sources.length}
				<p class="quiet">{i18n.t.noSources}</p>
			{:else if view}
				<ul class="source-list" data-testid="sources">
					{#each view.sources as source (source.instance)}
						<li class="source">
							<div class="source-text">
								<h3>{title(source)}</h3>
								<p class="quiet">{status(source)}</p>
								{#if source.last && source.last.outcome !== 'done'}
									<Notice tone="warn">{problem(source, source.last.code)}</Notice>
								{:else if source.last}
									<p>{runCounts(source.last)}</p>
								{/if}
								{#if source.followed && source.failures > 1}
									<p class="quiet">{i18n.t.failingSince(source.failures)}</p>
								{/if}
								{#if source.server}
									{@const server = source.server}
									{@const words = i18n.t.servers[source.connector as Server]}
									{#if server.plain}
										<p class="quiet">{words.plain}</p>
									{/if}
									{#if server.chosen}
										<p class="quiet">
											{i18n.t.followedWho(server.choices.find((c) => c.id === server.chosen)?.name ?? server.chosen)}
										</p>
										{#if server.positions !== null}
											<label class="check">
												<input
													type="checkbox"
													checked={server.positions}
													disabled={working !== null}
													aria-describedby={`positions-${source.instance}`}
													onchange={(e) => setPositions(source, e.currentTarget.checked)}
												/>
												{i18n.t.positionsLabel}
											</label>
											<p class="quiet" id={`positions-${source.instance}`}>{i18n.t.positionsHint}</p>
										{/if}
									{:else if server.choices.length}
										<h4>{i18n.t.chooseWho}</h4>
										<p class="quiet">{words.chooseHint}</p>
										<form class="collections" onsubmit={(e) => choosePerson(e, source)}>
											<ul>
												{#each server.choices as choice (choice.id)}
													<li>
														<label>
															<input
																type="radio"
																name={`choice-${source.instance}`}
																value={choice.id}
																checked={persons[source.instance] === choice.id}
																onchange={() => (persons[source.instance] = choice.id)}
															/>
															{choice.name}
														</label>
													</li>
												{/each}
											</ul>
											<button type="submit" class="button small primary" disabled={working !== null || !persons[source.instance]}>
												{working?.kind === 'choose' && working.instance === source.instance ? i18n.t.syncing : words.follow}
											</button>
										</form>
									{:else}
										<p class="quiet">{words.nobody}</p>
									{/if}
								{/if}
								{#if source.account}
									{@const account = source.account}
									{#snippet collections()}
										{#if account.collections.length}
											<form class="collections" onsubmit={(e) => choose(e, source)}>
												<ul>
													{#each account.collections as collection (collection.url)}
														<li>
															<label>
																<input
																	type="checkbox"
																	checked={picked(source).includes(collection.url)}
																	onchange={(e) => tick(source, collection.url, e.currentTarget.checked)}
																/>
																{collection.name}
																<span class="quiet">· {i18n.t.collectionKinds[collection.kind]}</span>
															</label>
														</li>
													{/each}
												</ul>
												<button type="submit" class="button small primary" disabled={working !== null}>
													{working?.kind === 'choose' && working.instance === source.instance ? i18n.t.syncing : i18n.t.followChosen}
												</button>
											</form>
										{:else}
											<p class="quiet">{i18n.t.noCollectionFound}</p>
										{/if}
									{/snippet}
									{#if account.chosen}
										<details class="exports">
											<summary>{i18n.t.followedCollections}</summary>
											{@render collections()}
										</details>
									{:else}
										<h4>{i18n.t.chooseCollections}</h4>
										<p class="quiet">{i18n.t.chooseCollectionsHint}</p>
										{@render collections()}
									{/if}
								{/if}
							</div>
							{#if source.followed}
								<div class="actions">
									<button
										type="button"
										class="button small"
										disabled={working !== null}
										onclick={() => run({ kind: 'sync', instance: source.instance }, () => api.syncSource(source.instance))}
										>{working?.kind === 'sync' && working.instance === source.instance ? i18n.t.syncing : i18n.t.syncNow}</button
									>
									<button type="button" class="button small" disabled={working !== null} onclick={() => unfollow(source)}
										>{i18n.t.unfollow}</button
									>
								</div>
							{/if}
						</li>
					{/each}
				</ul>
			{:else if screen.state.kind === 'failed'}
				<p class="quiet">{i18n.t.failed}</p>
			{/if}
		</section>

		<section aria-labelledby="add">
			<h2 id="add">{i18n.t.addSource}</h2>
			{#each CATEGORIES as category (category.id)}
				<div class="category" role="group" aria-labelledby={`category-${category.id}`}>
					<h3 id={`category-${category.id}`}>{i18n.t.sourceCategories[category.id]}</h3>
					<ul class="cards">
						{#each category.cards as card (card)}
							{@const words = i18n.t.sourceCards[card]}
							<li>
								<a class="source-card" href={`/sources?add=${card}`} onclick={() => (outcome = null)}>
									<svg viewBox="0 0 24 24" width="22" height="22" aria-hidden="true"><path d={ICONS[card]} /></svg>
									<span class="card-text">
										<span class="card-name">{words.name}</span>
										<span class="quiet">{words.line}</span>
									</span>
									{#if connected(card)}<span class="badge">{i18n.t.sourceConnected}</span>{/if}
								</a>
							</li>
						{/each}
					</ul>
				</div>
			{/each}
		</section>
	{/if}
</div>

<Confirm bind:question />
