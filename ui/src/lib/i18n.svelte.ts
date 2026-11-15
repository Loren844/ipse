// The words on screen, in French and English. The interface speaks the vault's language
// until the person picks the other one; the choice is kept in this browser only.

export type Language = 'fr' | 'en';

const plural = (n: number, one: string, many: string) => (n === 1 ? one : many.replace('#', String(n)));

const words = {
	fr: {
		appName: 'Ipse',
		memoryOf: 'Mémoire de',
		navLabel: 'Sections',
		home: 'Accueil',
		people: 'Personnes',
		agenda: 'Agenda',
		inbox: 'À revoir',
		ais: 'IA connectées',
		history: 'Historique',
		waiting: (n: number) => plural(n, '1 proposition à revoir', '# propositions à revoir'),
		comingUp: 'Ce qui vient',
		nothingComing: 'Rien de prévu dans les 30 prochains jours.',
		nothingComingHint: 'Importez votre agenda, ou abonnez-vous à son lien, pour voir ici ce qui arrive.',
		waitingBanner: (n: number) =>
			plural(n, '1 proposition attend votre avis', '# propositions attendent votre avis'),
		review: 'Les revoir',
		seenRecently: 'Vus récemment',
		yourFile: 'Votre fiche',
		search: 'Chercher une personne',
		searchHint: 'Un nom, un surnom, ou un mot de sa fiche',
		peopleCount: (n: number) => plural(n, '1 personne', '# personnes'),
		found: (n: number) => plural(n, '1 résultat', '# résultats'),
		nobodyFound: 'Personne ne correspond.',
		nobodyYet: 'Aucune personne dans votre mémoire pour l’instant.',
		nobodyYetHint: 'Importez vos contacts (un fichier .vcf) avec ipse import pour les voir ici.',
		showMore: 'Afficher plus',
		alsoCalled: (names: string) => `Autres noms : ${names}`,
		phone: 'Téléphone',
		email: 'Email',
		birthday: 'Anniversaire',
		address: 'Adresse',
		age: (n: number) => plural(n, '1 an', '# ans'),
		facts: 'Ce que vous savez',
		relations: 'Reliée à',
		onTheThread: 'Sur le fil',
		notes: 'Notes',
		noEvents: 'Aucun événement avec cette personne.',
		from: 'Vient de',
		sources: {
			contacts: 'vos contacts',
			calendar: 'votre agenda',
			proposal: 'une proposition acceptée',
			person: 'vous'
		} as Record<string, string>,
		sensitive: (category: string) =>
			`Pas visible par vos IA : la catégorie « ${category} » leur est fermée, sauf si vous l’ouvrez à l’une d’elles.`,
		categories: {
			identity: 'identité',
			people: 'personnes',
			agenda: 'agenda',
			places: 'lieux',
			media: 'médias',
			work: 'travail',
			things: 'objets',
			summaries: 'résumés',
			location: 'position',
			home: 'maison',
			health: 'santé',
			private: 'privé'
		} as Record<string, string>,
		hiddenFromAis: 'Pas visible par vos IA',
		with: 'Avec',
		place: 'Lieu',
		cancelled: 'Annulé',
		allDay: 'Toute la journée',
		today: 'Aujourd’hui',
		earlier: 'Plus tôt',
		later: 'Plus tard',
		noEventsPeriod: 'Aucun événement sur cette période.',
		back: 'Retour',
		notFound: 'Cette fiche n’existe pas, ou plus.',
		inboxEmpty: 'Rien à revoir pour le moment.',
		inboxEmptyHint:
			'Quand votre IA propose de compléter votre mémoire, ou qu’une de vos sources change une fiche que vous aviez modifiée, la proposition arrive ici. Rien n’est écrit sans votre accord.',
		inboxWaiting: (n: number) =>
			plural(
				n,
				'1 proposition attend votre décision. L’écran pour la revoir arrive bientôt ; en attendant : ipse proposals list.',
				'# propositions attendent votre décision. L’écran pour les revoir arrive bientôt ; en attendant : ipse proposals list.'
			),
		aisEmpty: 'Les IA que vous avez connectées apparaîtront ici, avec ce que chacune peut lire.',
		historyEmpty: 'Chaque changement de votre mémoire apparaîtra ici : qui l’a fait, et comment l’annuler.',
		language: 'Langue',
		loading: 'Ouverture de votre mémoire…',
		ended: 'Cette session est terminée.',
		endedHint: 'Pour rouvrir Ipse, lancez « ipse ui » sur cet ordinateur.',
		failed: 'Ipse ne répond pas.',
		failedHint: 'Vérifiez que « ipse ui » tourne toujours dans votre terminal, puis rechargez la page.'
	},
	en: {
		appName: 'Ipse',
		memoryOf: 'Memory of',
		navLabel: 'Sections',
		home: 'Home',
		people: 'People',
		agenda: 'Agenda',
		inbox: 'To review',
		ais: 'Connected AIs',
		history: 'History',
		waiting: (n: number) => plural(n, '1 proposal to review', '# proposals to review'),
		comingUp: 'Coming up',
		nothingComing: 'Nothing planned in the next 30 days.',
		nothingComingHint: 'Import your calendar, or subscribe to its link, to see here what is coming.',
		waitingBanner: (n: number) =>
			plural(n, '1 proposal waits for your decision', '# proposals wait for your decision'),
		review: 'Review them',
		seenRecently: 'Seen recently',
		yourFile: 'Your file',
		search: 'Search a person',
		searchHint: 'A name, a nickname, or a word from their file',
		peopleCount: (n: number) => plural(n, '1 person', '# people'),
		found: (n: number) => plural(n, '1 result', '# results'),
		nobodyFound: 'Nobody matches.',
		nobodyYet: 'No one in your memory yet.',
		nobodyYetHint: 'Import your contacts (a .vcf file) with ipse import to see them here.',
		showMore: 'Show more',
		alsoCalled: (names: string) => `Also known as ${names}`,
		phone: 'Phone',
		email: 'Email',
		birthday: 'Birthday',
		address: 'Address',
		age: (n: number) => plural(n, '1 year old', '# years old'),
		facts: 'What you know',
		relations: 'Linked to',
		onTheThread: 'On the thread',
		notes: 'Notes',
		noEvents: 'No events with this person.',
		from: 'From',
		sources: {
			contacts: 'your contacts',
			calendar: 'your calendar',
			proposal: 'an accepted proposal',
			person: 'you'
		} as Record<string, string>,
		sensitive: (category: string) =>
			`Not visible to your AIs: the “${category}” category is closed to them, unless you open it to one.`,
		categories: {
			identity: 'identity',
			people: 'people',
			agenda: 'agenda',
			places: 'places',
			media: 'media',
			work: 'work',
			things: 'things',
			summaries: 'summaries',
			location: 'location',
			home: 'home',
			health: 'health',
			private: 'private'
		} as Record<string, string>,
		hiddenFromAis: 'Not visible to your AIs',
		with: 'With',
		place: 'Place',
		cancelled: 'Cancelled',
		allDay: 'All day',
		today: 'Today',
		earlier: 'Earlier',
		later: 'Later',
		noEventsPeriod: 'No events in this period.',
		back: 'Back',
		notFound: 'This file does not exist, or no longer does.',
		inboxEmpty: 'Nothing to review right now.',
		inboxEmptyHint:
			'When your AI offers to complete your memory, or one of your sources changes a file you had edited, the proposal comes here. Nothing is written without your consent.',
		inboxWaiting: (n: number) =>
			plural(
				n,
				'1 proposal waits for your decision. The screen to review it comes soon; meanwhile: ipse proposals list.',
				'# proposals wait for your decision. The screen to review them comes soon; meanwhile: ipse proposals list.'
			),
		aisEmpty: 'The AIs you connected will show here, with what each one may read.',
		historyEmpty: 'Every change to your memory will show here: who made it, and how to undo it.',
		language: 'Language',
		loading: 'Opening your memory…',
		ended: 'This session has ended.',
		endedHint: 'To open Ipse again, run “ipse ui” on this computer.',
		failed: 'Ipse is not answering.',
		failedHint: 'Check that “ipse ui” is still running in your terminal, then reload the page.'
	}
};

export type Words = (typeof words)['fr'];

const KEY = 'ipse.language';

function stored(): Language | null {
	try {
		const value = localStorage.getItem(KEY);
		return value === 'fr' || value === 'en' ? value : null;
	} catch {
		return null;
	}
}

class I18n {
	language: Language = $state(stored() ?? 'fr');
	t: Words = $derived(words[this.language] as Words);
	#chosen = stored() !== null;

	/** The vault's language, unless the person chose one in this browser. */
	useVaultLanguage(language: string) {
		if (!this.#chosen && (language === 'fr' || language === 'en')) this.language = language;
	}

	choose(language: Language) {
		this.language = language;
		this.#chosen = true;
		try {
			localStorage.setItem(KEY, language);
		} catch {
			// Private windows may refuse: the choice then lasts for this page only.
		}
	}

	/** A category's name in the current language; a custom one as written. */
	category(name: string): string {
		return this.t.categories[name] ?? name;
	}
}

export const i18n = new I18n();
