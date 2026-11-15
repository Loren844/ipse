<script lang="ts">
	// Text to copy as it is: a code, a configuration, a command.
	import { i18n } from './i18n.svelte.ts';
	let { text, label }: { text: string; label: string } = $props();
	let copied = $state(false);

	async function copy() {
		try {
			await navigator.clipboard.writeText(text);
			copied = true;
			setTimeout(() => (copied = false), 2000);
		} catch {
			// Without clipboard access, the text stays selectable.
		}
	}
</script>

<div class="copy">
	<pre aria-label={label}>{text}</pre>
	<button type="button" class="button small" onclick={copy} aria-label="{i18n.t.copy} ({label})">
		<span aria-live="polite">{copied ? i18n.t.copied : i18n.t.copy}</span>
	</button>
</div>
