// Lints the interface (issue #287): JavaScript and TypeScript recommended rules, and
// Svelte's, which include its accessibility checks. Part of `npm run check`.
import js from '@eslint/js';
import svelte from 'eslint-plugin-svelte';
import globals from 'globals';
import ts from 'typescript-eslint';

export default ts.config(
	{ ignores: ['build/', '.svelte-kit/', 'node_modules/', 'test-results/', 'src/lib/types/'] },
	js.configs.recommended,
	...ts.configs.recommended,
	...svelte.configs.recommended,
	{
		languageOptions: { globals: { ...globals.browser, ...globals.node } },
		rules: {
			// The interface is served at the root of its own origin, never under a base
			// path: its links stay plain paths, without `resolve()`.
			'svelte/no-navigation-without-resolve': 'off',
			// The Maps and Sets it flags are built inside a `$derived` or a function and
			// returned whole: nothing changes them after, nothing needs to react.
			'svelte/prefer-svelte-reactivity': 'off',
			// TypeScript knows the DOM's types (`AutoFill`…), which this rule does not.
			'no-undef': 'off',
			// `_` names what is there on purpose and not used.
			'@typescript-eslint/no-unused-vars': ['error', { argsIgnorePattern: '^_', varsIgnorePattern: '^_' }]
		}
	},
	{
		// The tests read what `ipse mcp` answers as it comes.
		files: ['tests/**'],
		rules: { '@typescript-eslint/no-explicit-any': 'off' }
	},
	{
		files: ['**/*.svelte', '**/*.svelte.ts'],
		languageOptions: { parserOptions: { parser: ts.parser, extraFileExtensions: ['.svelte'] } }
	}
);
