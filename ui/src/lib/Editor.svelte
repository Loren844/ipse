<script lang="ts" module>
	import type { EditView } from './api.ts';

	/** An empty form for a new file of `kind`, its name filled in when known. */
	export function blank(kind: string, name = ''): EditView {
		const event = kind === 'event';
		return {
			id: '',
			kind,
			version: '',
			owner: false,
			name: event ? null : name,
			aliases: [],
			birthday: null,
			emails: [],
			phones: [],
			address: null,
			title: event ? name : null,
			start: null,
			end: null,
			place: null,
			participants: [],
			status: null,
			event_kind: null,
			facts: [],
			relations: [],
			notes: null,
			links: []
		};
	}
</script>

<script lang="ts">
	// The form that edits a file of the memory, or makes a new one: its fields, facts,
	// relations and notes. Saving sends everything at once; Ipse writes only what
	// changed, as one change the person can undo from the history.
	import { tick, untrack } from 'svelte';
	import { Conflict, SessionEnded, type Draft } from './api.ts';
	import { app } from './app.svelte.ts';
	import { i18n } from './i18n.svelte.ts';
	import LinkPicker, { type Picked } from './LinkPicker.svelte';
	import ListInput from './ListInput.svelte';
	import Notice from './Notice.svelte';

	let {
		form,
		creating = false,
		save,
		cancel,
		dirty = $bindable(false)
	}: { form: EditView; creating?: boolean; save: (draft: Draft) => Promise<void>; cancel: () => void; dirty?: boolean } = $props();

	// The form's own copy: the file as read stays in `form`, to send only what changed.
	const initial = untrack(() => $state.snapshot(form));
	const isNew = untrack(() => creating);
	const event = initial.kind === 'event';
	type Time = { date: string; time: string };
	const nextDay = (day: string, by: number) => {
		const d = new Date(`${day}T12:00:00Z`);
		d.setUTCDate(d.getUTCDate() + by);
		return d.toISOString().slice(0, 10);
	};
	const split = (text: string | null): Time => ({ date: text?.slice(0, 10) ?? '', time: text && text.length > 10 ? text.slice(11, 16) : '' });
	const startAt = split(initial.start);
	const wholeDay = !!initial.start && initial.start.length === 10;
	// A whole day's end is exclusive in the file; the form shows the last day.
	const endAt = (() => {
		const t = split(initial.end);
		return wholeDay && t.date ? { date: nextDay(t.date, -1), time: '' } : t;
	})();

	let name = $state(initial.name ?? '');
	let aliases = $state([...initial.aliases]);
	let birthday = $state(initial.birthday ?? '');
	let phones = $state([...initial.phones]);
	let emails = $state([...initial.emails]);
	let address = $state(initial.address ?? '');
	let title = $state(initial.title ?? '');
	let allDay = $state(isNew ? false : wholeDay);
	let start = $state({ ...startAt });
	let end = $state({ ...endAt });
	let place: Picked | null = $state(
		initial.place ? { value: initial.place.value, target: null, name: initial.place.link.name, id: initial.place.link.id, kind: initial.place.link.kind } : null
	);
	let people: Picked[] = $state(
		initial.participants.map((p) => ({ value: p.value, target: null, name: p.link.name, id: p.link.id, kind: p.link.kind }))
	);
	let status = $state(initial.status ?? '');
	let eventKind = $state(initial.event_kind ?? '');
	const EVENT_KINDS = ['meal', 'celebration', 'outing', 'culture', 'sport', 'trip', 'travel', 'work', 'study', 'appointment', 'health', 'other'];
	let facts = $state(initial.facts.map((text, was) => ({ was: was as number | null, text })));
	type Row = { word: string; target: string; display: string | null; name: string; key: number };
	let key = 0;
	let relations: Row[] = $state(
		initial.relations.map((r) => ({ word: r.word, target: r.target, display: r.display, name: r.link.name, key: key++ }))
	);
	let notes = $state(initial.notes ?? '');
	let busy = $state(false);
	let problem: string | null = $state(null);
	let factList: HTMLElement | undefined = $state();
	let relationList: HTMLElement | undefined = $state();

	const required = $derived(event ? !!title.trim() && !!start.date : !!name.trim());

	/** A time as the file keeps it: the original text when the fields did not change. */
	function timeOf(t: Time, was: Time, original: string | null, isEnd: boolean): string | null {
		if (!t.date) return null;
		if (!isNew && allDay === wholeDay && t.date === was.date && t.time === was.time) return original;
		if (allDay || !t.time) return isEnd && allDay ? nextDay(t.date, 1) : t.date;
		return `${t.date}T${t.time}`;
	}

	function draft(): Draft {
		const keys: Draft['keys'] = event
			? {
					title: title.trim(),
					start: timeOf(start, startAt, initial.start, false),
					end: timeOf(end, endAt, initial.end, true),
					place: place?.value ?? null,
					participants: people.map((p) => p.value),
					status: status || null,
					kind: eventKind || null
				}
			: {
					name: name.trim(),
					aliases,
					birthday: birthday.trim() || null,
					phones,
					emails,
					address: address.trim() || null
				};
		return {
			keys,
			facts: event ? undefined : facts,
			relations: event ? undefined : relations.filter((r) => r.target).map((r) => ({ word: r.word, target: r.target, display: r.display })),
			notes
		};
	}

	// Changed since it opened: switching to Markdown asks first (issue #142).
	const unchanged = untrack(() => JSON.stringify(draft()));
	$effect(() => {
		dirty = JSON.stringify(draft()) !== unchanged;
	});

	/** The server's refusal, in the person's words: which field, if it names one. */
	function refusal(message: string): string {
		if (message.includes('changed since')) return i18n.t.changedMeanwhile;
		const field = /`([a-z_]+)`/.exec(message)?.[1];
		const label = field ? i18n.t.keys[field] : null;
		return label ? i18n.t.cannotKeep(label) : i18n.t.cannotKeepFile;
	}

	async function submit(e: SubmitEvent) {
		e.preventDefault();
		if (!required) return;
		busy = true;
		problem = null;
		try {
			await save(draft());
		} catch (error) {
			if (error instanceof SessionEnded) app.state = { kind: 'ended' };
			problem = error instanceof Conflict ? refusal(error.message) : i18n.t.actionFailed;
		}
		busy = false;
	}

	async function addFact() {
		facts = [...facts, { was: null, text: '' }];
		await tick();
		factList?.querySelector<HTMLInputElement>('li:last-child input')?.focus();
	}

	async function addRelation() {
		relations = [...relations, { word: '', target: '', display: null, name: '', key: key++ }];
		await tick();
		relationList?.querySelector<HTMLInputElement>('li:last-child input')?.focus();
	}

	async function unpick(row: Row) {
		row.target = '';
		row.name = '';
		row.display = null;
		await tick();
		document.getElementById(`relation-${row.key}-to`)?.focus();
	}
</script>

<form class="editor" onsubmit={submit} novalidate>
	{#if event}
		<div class="form-field">
			<label for="edit-title">{i18n.t.keys.title}</label>
			<input id="edit-title" type="text" bind:value={title} required aria-describedby={title.trim() ? undefined : 'title-required'} />
			{#if !title.trim()}<p class="hint" id="title-required">{i18n.t.required}</p>{/if}
		</div>
		<div class="edit-when">
			<label class="check"><input type="checkbox" bind:checked={allDay} /> {i18n.t.allDay}</label>
			<div class="pair">
				<div class="form-field">
					<label for="edit-start">{i18n.t.keys.start}</label>
					<input id="edit-start" type="date" bind:value={start.date} required />
				</div>
				{#if !allDay}
					<div class="form-field">
						<label for="edit-start-time">{i18n.t.startTime}</label>
						<input id="edit-start-time" type="time" bind:value={start.time} />
					</div>
				{/if}
			</div>
			<div class="pair">
				<div class="form-field">
					<label for="edit-end">{i18n.t.keys.end}</label>
					<input id="edit-end" type="date" bind:value={end.date} min={start.date || undefined} />
				</div>
				{#if !allDay}
					<div class="form-field">
						<label for="edit-end-time">{i18n.t.endTime}</label>
						<input id="edit-end-time" type="time" bind:value={end.time} />
					</div>
				{/if}
			</div>
		</div>
		<div class="form-field">
			<label for="edit-kind">{i18n.t.keys.kind}</label>
			<select id="edit-kind" bind:value={eventKind}>
				<option value="">{i18n.t.eventKindNone}</option>
				{#each EVENT_KINDS as kind (kind)}<option value={kind}>{i18n.t.eventKinds[kind]}</option>{/each}
			</select>
		</div>
		<fieldset class="status">
			<legend>{i18n.t.keys.status}</legend>
			<div class="segmented">
				{#each ['', 'planned', 'done', 'cancelled'] as choice (choice)}
					<button type="button" aria-pressed={status === choice} onclick={() => (status = choice)}>{i18n.t.statuses[choice]}</button>
				{/each}
			</div>
		</fieldset>
		<div class="form-field">
			<span class="label" id="place-label">{i18n.t.keys.place}</span>
			{#if place}
				<p class="chosen">
					<span>{place.name}</span>
					<button type="button" class="remove" aria-label={i18n.t.removeItem(place.name)} onclick={() => (place = null)}
						><svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true"><path d="M6 6l12 12M18 6 6 18" /></svg></button
					>
				</p>
			{:else}
				<LinkPicker id="edit-place" label={i18n.t.keys.place} free kinds={['place', 'organization']} onpick={(p) => (place = p)} />
			{/if}
		</div>
		<fieldset class="list-input">
			<legend>{i18n.t.keys.participants}</legend>
			{#if people.length}
				<ul class="chips">
					{#each people as person, i (person.value + i)}
						<li class="chosen">
							<span>{person.name}</span>
							<button
								type="button"
								class="remove"
								aria-label={i18n.t.removeItem(person.name)}
								onclick={() => (people = people.filter((_, j) => j !== i))}
								><svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true"><path d="M6 6l12 12M18 6 6 18" /></svg></button
							>
						</li>
					{/each}
				</ul>
			{/if}
			<LinkPicker
				id="edit-people"
				label={i18n.t.addParticipant}
				free
				kinds={['person', 'organization']}
				onpick={(p) => {
					if (!people.some((q) => q.value === p.value)) people = [...people, p];
				}}
			/>
		</fieldset>
	{:else}
		<div class="form-field">
			<label for="edit-name">{i18n.t.keys.name}</label>
			<input id="edit-name" type="text" bind:value={name} required autocomplete="off" />
			{#if !name.trim()}<p class="hint">{i18n.t.required}</p>{/if}
		</div>
		<ListInput id="edit-aliases" label={i18n.t.keys.aliases} add={i18n.t.addAlias} bind:values={aliases} />
		{#if initial.kind === 'person'}
			<ListInput id="edit-phones" label={i18n.t.keys.phones} add={i18n.t.addPhone} type="tel" bind:values={phones} />
			<ListInput id="edit-emails" label={i18n.t.keys.emails} add={i18n.t.addEmail} type="email" bind:values={emails} />
			<div class="form-field">
				<label for="edit-birthday">{i18n.t.keys.birthday}</label>
				<input id="edit-birthday" type="text" inputmode="numeric" bind:value={birthday} aria-describedby="birthday-hint" placeholder="1990-04-02" />
				<p class="hint" id="birthday-hint">{i18n.t.birthdayHint}</p>
			</div>
		{/if}
		{#if initial.kind === 'person' || initial.kind === 'place' || initial.kind === 'organization'}
			<div class="form-field">
				<label for="edit-address">{i18n.t.keys.address}</label>
				<input id="edit-address" type="text" bind:value={address} autocomplete="off" />
			</div>
		{/if}

		<fieldset class="list-input facts-input">
			<legend>{i18n.t.facts}</legend>
			<p class="hint">{i18n.t.factsHint}</p>
			<ul bind:this={factList}>
				{#each facts as fact, i (i)}
					<li>
						<input type="text" aria-label="{i18n.t.fact} {i + 1}" bind:value={fact.text} />
						<button
							type="button"
							class="remove"
							aria-label={i18n.t.removeItem(fact.text || `${i18n.t.fact} ${i + 1}`)}
							onclick={() => (facts = facts.filter((_, j) => j !== i))}
							><svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true"><path d="M6 6l12 12M18 6 6 18" /></svg></button
						>
					</li>
				{/each}
			</ul>
			<button type="button" class="link-button add" onclick={addFact}>+ {i18n.t.addFact}</button>
		</fieldset>

		<fieldset class="list-input relations-input">
			<legend>{i18n.t.relations}</legend>
			<ul bind:this={relationList}>
				{#each relations as row (row.key)}
					<li>
						<input
							type="text"
							aria-label={i18n.t.relationWord}
							placeholder={i18n.t.relationWordHint}
							bind:value={row.word}
						/>
						{#if row.target}
							<span class="chosen">
								<span>{row.name}</span>
								<button type="button" class="remove" aria-label={i18n.t.removeItem(row.name)} onclick={() => unpick(row)}
									><svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true"><path d="M6 6l12 12M18 6 6 18" /></svg></button
								>
							</span>
						{:else}
							<LinkPicker
								id="relation-{row.key}-to"
								label={i18n.t.relationTarget}
								kinds={['person', 'place', 'organization', 'project', 'work', 'thing', 'topic']}
								onpick={(p) => {
									row.target = p.target ?? '';
									row.name = p.name;
									row.display = null;
								}}
							/>
						{/if}
						<button
							type="button"
							class="remove"
							aria-label={i18n.t.removeItem(`${row.word} ${row.name}`.trim() || i18n.t.relation)}
							onclick={() => (relations = relations.filter((r) => r.key !== row.key))}
							><svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true"><path d="M6 6l12 12M18 6 6 18" /></svg></button
						>
					</li>
				{/each}
			</ul>
			<button type="button" class="link-button add" onclick={addRelation}>+ {i18n.t.addRelation}</button>
		</fieldset>
	{/if}

	<div class="form-field">
		<label for="edit-notes">{i18n.t.notes}</label>
		<textarea id="edit-notes" rows="5" bind:value={notes} aria-describedby="notes-hint"></textarea>
		<p class="hint" id="notes-hint">{i18n.t.notesHint}</p>
	</div>

	<div class="live" role="status" aria-live="polite">
		{#if problem}<Notice tone="warn">{problem}</Notice>{/if}
	</div>

	<div class="actions">
		<button type="submit" class="button primary" disabled={busy || !required}>{creating ? i18n.t.createFile : i18n.t.saveFile}</button>
		<button type="button" class="button" onclick={cancel}>{i18n.t.cancelForm}</button>
	</div>
</form>
