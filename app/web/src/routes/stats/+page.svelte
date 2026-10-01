<script lang="ts">
	import { onMount } from 'svelte';
	import DailyProgress from '$lib/DailyProgress.svelte';
	import { loadProgress, progress } from '$lib/progress.svelte';
	let error = $state('');
	onMount(async () => {
		if (!(await loadProgress())) error = 'Could not load your progress. Try refreshing the page.';
	});
</script>

<svelte:head><title>Your progress · BookReplay</title></svelte:head>
<main id="main-content" class="review-main page-enter">
	<p class="eyebrow mt-6">Your practice, taking root</p>
	<h1 class="mt-4 font-serif text-5xl tracking-tight">Small visits add up.</h1>
	<p class="intro-copy">Every idea you return to is a little more likely to stay with you.</p>
	{#if error}
		<p role="alert" class="text-danger">{error}</p>
	{:else if progress.loaded}
		<DailyProgress count={progress.dailyRevisionCount} streak={progress.streak} />
		<section class="mt-8 rounded-xl border border-line p-6" aria-labelledby="habit-title">
			<h2 id="habit-title" class="font-serif text-2xl">Make a little room for reading.</h2>
			<p class="mt-3 leading-relaxed text-muted">Revisit five highlights in a day to grow your streak. Come back on consecutive days to keep it going. If you miss a day, you can always begin again.</p>
			<p class="mt-3 text-xs text-muted">Daily progress resets at midnight UTC.</p>
		</section>
		<a class="primary-button mt-8" href="/review/">{progress.dailyRevisionCount >= 5 ? 'Revisit a few more' : 'Take a moment to review'} →</a>
	{:else}
		<p role="status" class="text-muted">Gathering your progress…</p>
	{/if}
</main>
