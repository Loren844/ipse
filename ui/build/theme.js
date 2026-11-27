// Sets the theme before the page paints, so that it never flashes: the person's choice,
// kept in this browser by the settings screen, or else the system's. A file, not an
// inline script, so that the page keeps its strict Content-Security-Policy.
(function () {
	var root = document.documentElement;
	var media = window.matchMedia('(prefers-color-scheme: dark)');
	function apply() {
		var chosen = null;
		try {
			chosen = localStorage.getItem('ipse.theme');
		} catch {
			// Private windows may refuse: the system's theme then applies.
		}
		var dark = chosen === 'dark' || (chosen !== 'light' && media.matches);
		root.setAttribute('data-theme', dark ? 'dark' : 'light');
	}
	apply();
	media.addEventListener('change', apply);
	window.addEventListener('ipse-theme', apply);
})();
