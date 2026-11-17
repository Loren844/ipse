// The app's first screen: open the memory chosen last, or choose or make one.
// The app's commands are in app/src/commands.rs.
'use strict';

const words = {
	fr: {
		opening: 'Ouverture de votre mémoire…',
		title: 'La mémoire de votre vie, sur cet ordinateur.',
		lead: 'Ipse la garde dans un dossier de fichiers que vous possédez. Vos IA y lisent ce que vous leur ouvrez, et ne changent rien sans votre accord.',
		reopen: 'Rouvrir ma mémoire',
		create: 'Créer ma mémoire',
		createHint: 'Un dossier neuf, à votre nom. Vous y importerez ensuite vos contacts, votre agenda et vos lieux.',
		name: 'Votre nom',
		folder: 'Dossier',
		pick: 'Choisir…',
		folderHint: 'Un dossier vide ou nouveau. Vous pourrez l’ouvrir aussi dans Obsidian ou tout éditeur de texte.',
		language: 'Langue de la mémoire',
		zone: 'Fuseau horaire',
		createGo: 'Créer ma mémoire',
		demo: 'Essayer avec une mémoire inventée',
		demoHint: 'Environ 150 personnes imaginaires et deux ans d’agenda, pour voir Ipse sans vos données.',
		demoGo: 'Créer la mémoire inventée',
		open: 'Ouvrir une mémoire existante',
		openHint: 'Un dossier déjà créé par Ipse, sur cet ordinateur ou synchronisé depuis un autre.',
		creating: 'Création…',
		opening2: 'Ouverture…',
		ways: 'Choix',
		failed: 'Ipse n’a pas pu répondre. Quittez-le et rouvrez-le.'
	},
	en: {
		opening: 'Opening your memory…',
		title: 'The memory of your life, on this computer.',
		lead: 'Ipse keeps it in a folder of files you own. Your AIs read what you open to them, and change nothing without your OK.',
		reopen: 'Open my memory again',
		create: 'Create my memory',
		createHint: 'A new folder, in your name. You will then import your contacts, your calendar and your places.',
		name: 'Your name',
		folder: 'Folder',
		pick: 'Choose…',
		folderHint: 'An empty or new folder. You can open it in Obsidian or any text editor too.',
		language: 'Language of the memory',
		zone: 'Time zone',
		createGo: 'Create my memory',
		demo: 'Try it with an invented memory',
		demoHint: 'About 150 made-up people and two years of calendar, to see Ipse without your own data.',
		demoGo: 'Create the invented memory',
		open: 'Open an existing memory',
		openHint: 'A folder Ipse already made, on this computer or synced from another.',
		creating: 'Creating…',
		opening2: 'Opening…',
		ways: 'Choices',
		failed: 'Ipse could not answer. Quit it and open it again.'
	}
};

const tauri = window.__TAURI__;
const $ = (id) => document.getElementById(id);
let t = words.fr;

/** Shows the screen's words in `language`. */
function speak(language) {
	t = words[language] ?? words.en;
	document.documentElement.lang = language;
	for (const element of document.querySelectorAll('[data-words]')) {
		element.textContent = t[element.dataset.words];
	}
	document.querySelector('.ways').setAttribute('aria-label', t.ways);
}

/** Says `message` in `element`, or hides it when empty. */
function say(element, message) {
	element.textContent = message ?? '';
	element.hidden = !message;
}

/** Runs `work` with `button` busy (saying `label`, when given), showing a failure in
 * `error`. */
async function busy(button, label, error, work) {
	const before = button.textContent;
	button.disabled = true;
	button.setAttribute('aria-busy', 'true');
	if (label) button.textContent = label;
	say(error, '');
	try {
		// The app takes the window to the memory: the button stays busy until then.
		await work();
	} catch (failure) {
		say(error, typeof failure === 'string' ? failure : t.failed);
		button.disabled = false;
		button.removeAttribute('aria-busy');
		if (label) button.textContent = before;
	}
}

/** Every time zone the webview knows, the computer's first. */
function fillZones() {
	const here = Intl.DateTimeFormat().resolvedOptions().timeZone || 'UTC';
	let zones = [];
	try {
		zones = Intl.supportedValuesOf('timeZone');
	} catch {
		zones = [];
	}
	if (!zones.includes(here)) zones.unshift(here);
	const select = $('zone');
	for (const zone of zones) {
		const option = document.createElement('option');
		option.value = zone;
		option.textContent = zone.replaceAll('_', ' ');
		select.append(option);
	}
	select.value = here;
	return here;
}

/** Opens one way in and closes the others. */
function choose(way) {
	for (const button of document.querySelectorAll('.way[aria-controls]')) {
		const open = button.dataset.way === way && button.getAttribute('aria-expanded') !== 'true';
		button.setAttribute('aria-expanded', String(open));
		$(button.getAttribute('aria-controls')).hidden = !open;
		if (open) $(button.getAttribute('aria-controls')).querySelector('input')?.focus();
	}
}

async function pickFolder(input) {
	const picked = await tauri.dialog.open({ directory: true, defaultPath: input.value || undefined });
	if (typeof picked === 'string') input.value = picked;
}

async function start() {
	const choosing = new URLSearchParams(window.location.search).has('choose');
	let answer;
	try {
		answer = await tauri.core.invoke('start', { choose: choosing });
	} catch {
		speak(navigator.language.toLowerCase().startsWith('fr') ? 'fr' : 'en');
		say($('opening').querySelector('p'), t.failed);
		return;
	}
	// Opened: the app is taking the window to the memory.
	if (answer.kind === 'opened') return;
	speak(answer.language);
	const zone = fillZones();
	$('language').value = answer.language;
	$('folder').value = answer.folder;
	$('demo-folder').value = answer.demo_folder;
	say($('problem'), answer.problem);
	if (answer.last) {
		$('last').hidden = false;
		$('last-folder').textContent = answer.last;
		$('reopen').addEventListener('click', (event) =>
			busy(event.currentTarget, t.opening2, $('problem'), () =>
				tauri.core.invoke('open', { folder: answer.last })
			)
		);
	}

	for (const button of document.querySelectorAll('.way[aria-controls]')) {
		button.addEventListener('click', () => choose(button.dataset.way));
	}
	for (const button of document.querySelectorAll('[data-pick]')) {
		button.addEventListener('click', () => pickFolder($(button.dataset.pick)));
	}
	$('create').addEventListener('submit', (event) => {
		event.preventDefault();
		const form = event.currentTarget;
		busy(form.querySelector('[type=submit]'), t.creating, form.querySelector('.error'), () =>
			tauri.core.invoke('create', {
				new: {
					name: $('name').value.trim(),
					language: $('language').value,
					time_zone: $('zone').value || zone,
					folder: $('folder').value.trim(),
					demo: false
				}
			})
		);
	});
	$('demo').addEventListener('submit', (event) => {
		event.preventDefault();
		const form = event.currentTarget;
		busy(form.querySelector('[type=submit]'), t.creating, form.querySelector('.error'), () =>
			tauri.core.invoke('create', {
				new: {
					name: null,
					language: answer.language,
					time_zone: zone,
					folder: $('demo-folder').value.trim(),
					demo: true
				}
			})
		);
	});
	document.querySelector('[data-way=open]').addEventListener('click', async (event) => {
		const button = event.currentTarget;
		const picked = await tauri.dialog.open({ directory: true });
		if (typeof picked !== 'string') return;
		busy(button, null, $('open-error'), () => tauri.core.invoke('open', { folder: picked }));
	});

	$('opening').hidden = true;
	$('choose').hidden = false;
}

start();
