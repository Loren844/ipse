<script lang="ts">
	// What feeds the memory (issue #152): files imported, calendars followed by link,
	// calendars and contacts accounts (CalDAV, CardDAV: issue #154), Home Assistant
	// (issue #225).
	import FirstVisit from '#lib/FirstVisit.svelte';
	import { api, SessionEnded, type SourcesChange, type SourcesView, type SourceView } from '#lib/api.ts';
	import { app } from '#lib/app.svelte.ts';
	import Confirm, { type Question } from '#lib/Confirm.svelte';
	import { ago, runCounts } from '#lib/format.ts';
	import { i18n } from '#lib/i18n.svelte.ts';
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
		| { kind: 'connect-home' }
		| { kind: 'sync'; instance: string }
		| { kind: 'choose'; instance: string }
		| null = $state(null);
	let over = $state(false);
	let picker: HTMLInputElement | undefined = $state();
	let calendarName = $state('');
	let link = $state('');
	let accountName = $state('');
	let address = $state('');
	let user = $state('');
	let password = $state('');
	let homeName = $state('');
	let homeAddress = $state('');
	let token = $state('');
	/** The person picked on each Home Assistant, before it is sent. */
	let persons: Record<string, string> = $state({});
	/** The collections ticked on each account, before they are sent. */
	let picks: Record<string, string[]> = $state({});

	const isAccount = (source: SourceView) => source.connector === 'dav';
	const isHome = (source: SourceView) => source.connector === 'homeassistant';
	const picked = (source: SourceView) =>
		picks[source.instance] ?? source.account?.collections.filter((c) => c.followed).map((c) => c.url) ?? [];

	function tick(source: SourceView, url: string, on: boolean) {
		const now = picked(source).filter((u) => u !== url);
		picks[source.instance] = on ? [...now, url] : now;
	}

	/** Why a source failed, in the words that fit it. */
	function problem(source: SourceView, code: string | null): string {
		const t = i18n.t;
		const own = isAccount(source) ? t.accountProblems : isHome(source) ? t.homeProblems : {};
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
						working?.kind === 'connect' || working?.kind === 'connect-home'
							? t.alreadyConnected(change.detail ?? '')
							: t.alreadyFollowed(change.detail ?? '')
				};
			case 'connected':
				return { tone: 'done', text: working?.kind === 'connect-home' ? t.homeConnectedNow(name) : t.connectedNow(name) };
			case 'bad_address':
				return { tone: 'warn', text: working?.kind === 'connect-home' ? t.homeBadAddress : t.sourceOutcomes.bad_address };
			case 'not_connected': {
				const why = (working?.kind === 'connect-home' ? t.homeProblems : t.accountProblems)[change.detail ?? ''];
				return { tone: 'warn', text: why ?? t.sourceOutcomes.not_connected };
			}
			case 'imported':
			case 'synced':
				return { tone: 'done', text: t.sourceOutcomes[change.outcome] };
			case 'failed': {
				const source = change.sources.sources.find((s) => s.instance === change.source);
				const why = source && (isAccount(source) || isHome(source)) ? problem(source, change.detail) : t.runProblems[change.detail ?? ''];
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
		run({ kind: 'import', name: file.name }, () => api.importFile(file));
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
		}
	}

	async function connectHome(event: SubmitEvent) {
		event.preventDefault();
		const change = await run({ kind: 'connect-home' }, () => api.connectHome(homeAddress.trim(), token, homeName.trim()));
		// The token leaves the page once it is kept.
		if (change?.source) {
			token = '';
			homeAddress = '';
			homeName = '';
		}
	}

	async function choosePerson(event: SubmitEvent, source: SourceView) {
		event.preventDefault();
		const person = persons[source.instance];
		if (!person) return;
		const change = await run({ kind: 'choose', instance: source.instance }, () => api.choosePerson(source.instance, person));
		if (change) delete persons[source.instance];
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
		const body = isAccount(source) ? t.unfollowAccountBody : isHome(source) ? t.unfollowHomeBody : t.unfollowBody;
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
		{:else if working?.kind === 'connect-home'}
			<p class="quiet">{i18n.t.homeConnecting}</p>
		{:else if outcome}
			<Notice tone={outcome.tone}>{outcome.text}</Notice>
		{/if}
	</div>

	<div class="add">
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

		<section aria-labelledby="home-assistant">
			<h2 id="home-assistant">{i18n.t.homeTitle}</h2>
			<p class="quiet" id="home-hint">{i18n.t.homeHint}</p>
			<form class="setting-form" onsubmit={connectHome}>
				<div class="form-field">
					<label for="home-name">{i18n.t.homeName}</label>
					<input id="home-name" type="text" bind:value={homeName} autocomplete="off" />
				</div>
				<div class="form-field">
					<label for="home-address">{i18n.t.homeAddress}</label>
					<input
						id="home-address"
						type="text"
						inputmode="url"
						bind:value={homeAddress}
						required
						autocomplete="off"
						spellcheck="false"
						aria-describedby="home-hint"
					/>
				</div>
				<div class="form-field">
					<label for="home-token">{i18n.t.homeToken}</label>
					<input id="home-token" type="password" bind:value={token} required autocomplete="off" />
				</div>
				<div class="actions">
					<button type="submit" class="button primary" disabled={working !== null || !homeAddress.trim() || !token}>
						{working?.kind === 'connect-home' ? i18n.t.homeConnecting : i18n.t.homeGo}
					</button>
				</div>
			</form>
			<details class="exports">
				<summary>{i18n.t.homeTokenHelp}</summary>
				{#each i18n.t.homeTokenSteps as step (step.title)}
					<h3>{step.title}</h3>
					<ul>
						{#each step.lines as line (line)}<li>{line}</li>{/each}
					</ul>
				{/each}
			</details>
		</section>
	</div>

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
							{#if source.home}
								{@const home = source.home}
								{#if home.plain}
									<p class="quiet">{i18n.t.homePlain}</p>
								{/if}
								{#if home.chosen}
									<p class="quiet">
										{i18n.t.followedPerson(home.people.find((p) => p.entity === home.chosen)?.name ?? home.chosen)}
									</p>
								{:else if home.people.length}
									<h4>{i18n.t.choosePerson}</h4>
									<p class="quiet">{i18n.t.choosePersonHint}</p>
									<form class="collections" onsubmit={(e) => choosePerson(e, source)}>
										<ul>
											{#each home.people as person (person.entity)}
												<li>
													<label>
														<input
															type="radio"
															name={`person-${source.instance}`}
															value={person.entity}
															checked={persons[source.instance] === person.entity}
															onchange={() => (persons[source.instance] = person.entity)}
														/>
														{person.name}
													</label>
												</li>
											{/each}
										</ul>
										<button type="submit" class="button small primary" disabled={working !== null || !persons[source.instance]}>
											{working?.kind === 'choose' && working.instance === source.instance ? i18n.t.syncing : i18n.t.followPerson}
										</button>
									</form>
								{:else}
									<p class="quiet">{i18n.t.noPersonFound}</p>
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
</div>

<Confirm bind:question />
