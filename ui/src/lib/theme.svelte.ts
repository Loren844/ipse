// The theme the person chose, kept in this browser; `static/theme.js` applies it before
// the page paints, and again whenever it changes.

export type ThemeChoice = 'system' | 'light' | 'dark';

const KEY = 'ipse.theme';

function stored(): ThemeChoice {
	try {
		const value = localStorage.getItem(KEY);
		return value === 'light' || value === 'dark' ? value : 'system';
	} catch {
		return 'system';
	}
}

class Theme {
	choice: ThemeChoice = $state(stored());

	choose(choice: ThemeChoice) {
		this.choice = choice;
		try {
			if (choice === 'system') localStorage.removeItem(KEY);
			else localStorage.setItem(KEY, choice);
		} catch {
			// Private windows may refuse: the choice then lasts for this page only.
			document.documentElement.setAttribute(
				'data-theme',
				choice === 'system' ? (matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light') : choice
			);
			return;
		}
		window.dispatchEvent(new Event('ipse-theme'));
	}
}

export const theme = new Theme();
