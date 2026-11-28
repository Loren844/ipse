<script lang="ts">
	// The first steps on the home screen (issue #172): each a knot on the thread, past
	// when the memory shows it done, filled for the one to do now. With an invented
	// memory, how to make one's own instead.
	import { api, type GuideView } from './api.ts';
	import { appLinks, inApp } from './app.svelte.ts';
	import Copy from './Copy.svelte';
	import { i18n } from './i18n.svelte.ts';

	let { guide }: { guide: GuideView } = $props();
	let hidden = $state(false);

	const LINKS: Record<string, string> = { sources: '/sources', ai: '/ais', review: '/review' };
	const done = $derived(guide.steps.filter((s) => s.done).length);
	const next = $derived(guide.steps.find((s) => !s.done)?.id);

	async function hide() {
		hidden = true;
		try {
			await api.hideFirstSteps();
		} catch {
			// Shown again at the next visit: nothing is lost.
		}
	}
</script>

{#if !hidden}
	<section class="first-steps" aria-labelledby="first-steps" data-testid="first-steps">
		<div class="first-steps-head">
			<h2 id="first-steps">{guide.demo ? i18n.t.demoTitle : i18n.t.firstSteps}</h2>
			{#if !guide.demo}<span class="steps-count">{i18n.t.firstStepsCount(done, guide.steps.length)}</span>{/if}
			<a class="how-link" href="/how">{i18n.t.how.title}</a>
			<button type="button" class="button small" onclick={hide}>{i18n.t.firstStepsHide}</button>
		</div>
		{#if guide.demo}
			<div class="step-text">
				<p>{i18n.t.demoText}</p>
				{#if inApp}
					<p><a class="button" href={appLinks.switchMemory}>{i18n.t.demoCreate}</a></p>
				{:else}
					<p>{i18n.t.demoCommand}</p>
					<Copy text="ipse init ~/Ipse" label={i18n.t.demoCreate} />
				{/if}
			</div>
		{:else}
			<ol aria-label={i18n.t.firstStepsList}>
				{#each guide.steps as step (step.id)}
					{@const words = i18n.t.steps[step.id]}
					<li class:done={step.done} class:now={step.id === next} data-step={step.id}>
						<h3>
							{words.title}{#if step.done}<span class="visually-hidden"> ({i18n.t.stepDone})</span>{/if}
						</h3>
						{#if !step.done}
							<p>{words.text}</p>
							{#if step.id === 'question'}
								<Copy text={i18n.t.firstQuestion} label={i18n.t.firstQuestionLabel} />
								<p class="first-prompts" data-testid="first-prompts">
									{i18n.t.firstPrompts}
									{#each i18n.t.firstPromptNames as name, i (name)}{i ? ', ' : ' '}<q>{name}</q>{/each}.
								</p>
							{:else}
								<p><a class="button" class:primary={step.id === next} href={LINKS[step.id]}>{words.action}</a></p>
							{/if}
						{/if}
					</li>
				{/each}
			</ol>
		{/if}
	</section>
{/if}
