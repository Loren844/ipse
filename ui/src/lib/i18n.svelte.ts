// The words on screen, in French and English. The interface speaks the vault's language
// until the person picks the other one; the choice is kept in this browser only.

export type Language = 'fr' | 'en';

const words = {
	fr: {
		appName: 'Ipse',
		memoryOf: 'Mémoire de',
		navLabel: 'Sections',
		inbox: 'À revoir',
		ais: 'IA connectées',
		history: 'Historique',
		waiting: (n: number) => (n === 1 ? '1 proposition à revoir' : `${n} propositions à revoir`),
		inboxEmpty: 'Rien à revoir pour le moment.',
		inboxEmptyHint:
			'Quand votre IA propose de compléter votre mémoire, ou qu’une de vos sources change une fiche que vous aviez modifiée, la proposition arrive ici. Rien n’est écrit sans votre accord.',
		inboxWaiting: (n: number) =>
			n === 1
				? '1 proposition attend votre décision. L’écran pour la revoir arrive bientôt ; en attendant : ipse proposals list.'
				: `${n} propositions attendent votre décision. L’écran pour les revoir arrive bientôt ; en attendant : ipse proposals list.`,
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
		inbox: 'To review',
		ais: 'Connected AIs',
		history: 'History',
		waiting: (n: number) => (n === 1 ? '1 proposal to review' : `${n} proposals to review`),
		inboxEmpty: 'Nothing to review right now.',
		inboxEmptyHint:
			'When your AI offers to complete your memory, or one of your sources changes a file you had edited, the proposal comes here. Nothing is written without your consent.',
		inboxWaiting: (n: number) =>
			n === 1
				? '1 proposal waits for your decision. The screen to review it comes soon; meanwhile: ipse proposals list.'
				: `${n} proposals wait for your decision. The screen to review them comes soon; meanwhile: ipse proposals list.`,
		aisEmpty: 'The AIs you connected will show here, with what each one may read.',
		historyEmpty: 'Every change to your memory will show here: who made it, and how to undo it.',
		language: 'Language',
		loading: 'Opening your memory…',
		ended: 'This session has ended.',
		endedHint: 'To open Ipse again, run “ipse ui” on this computer.',
		failed: 'Ipse is not answering.',
		failedHint: 'Check that “ipse ui” is still running in your terminal, then reload the page.'
	}
} as const;

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
}

export const i18n = new I18n();
