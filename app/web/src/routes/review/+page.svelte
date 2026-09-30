<script lang="ts">
	import { onMount, tick } from 'svelte';
	import DailyProgress from '$lib/DailyProgress.svelte';

	type Rating = 'SOON' | 'LATER' | 'MUCH_LATER' | 'ARCHIVE';

	type Highlight = {
		highlight_id: number;
		book_title: string;
		authors: string[];
		highlight_text: string;
	};

	type ReviewResponse = {
		streak: number;
		daily_revision_count: number;
		goal_reached: boolean;
		next_interval_days: number | null;
	};

	let highlights = $state<Highlight[]>([]);
	let currentIndex = $state(0);
	let loading = $state(true);
	let submitting = $state(false);
	let error = $state('');
	let feedback = $state('');
	let progressLoaded = $state(false);
	let milestoneTitle = $state<HTMLHeadingElement>();
	let streak = $state(0);
	let dailyRevisionCount = $state(0);
	let goalReached = $state(false);
	let totals = $state<Record<Rating, number>>({
		SOON: 0,
		LATER: 0,
		MUCH_LATER: 0,
		ARCHIVE: 0
	});
	let current = $derived(highlights[currentIndex]);
	let complete = $derived(highlights.length > 0 && currentIndex === highlights.length);

	onMount(async () => {
		const loadProgress = fetch('/api/reviews/streak').then(async (response) => {
			if (!response.ok) return;
			const progress = await response.json();
			streak = progress.streak;
			dailyRevisionCount = progress.daily_revision_count;
			progressLoaded = true;
		}).catch(() => {});
		try {
			const response = await fetch('/api/reviews/highlights/session?limit=100');
			if (!response.ok) throw new Error();
			highlights = (await response.json()) as Highlight[];
		} catch {
			error = 'Could not load your review session. Try refreshing the page.';
		} finally {
			await loadProgress;
			loading = false;
		}
	});

	async function review(rating: Rating) {
		if (!current || submitting) return;

		submitting = true;
		error = '';
		try {
			const response = await fetch(`/api/reviews/highlights/${current.highlight_id}`, {
				method: 'POST',
				headers: { 'content-type': 'application/json' },
				body: JSON.stringify({ rating })
			});
			if (!response.ok) throw new Error();
			const progress = (await response.json()) as ReviewResponse;
			streak = progress.streak;
			window.dispatchEvent(new CustomEvent('bookreplay:streak', { detail: streak }));
			dailyRevisionCount = progress.daily_revision_count;
			goalReached = progress.goal_reached;
			progressLoaded = true;
			feedback = rating === 'ARCHIVE' ? 'Archived. Kept in your library.' : `Saved. Returning in ${progress.next_interval_days} days.`;

			totals = { ...totals, [rating]: totals[rating] + 1 };
			currentIndex += 1;
			await tick();
			if (goalReached || complete) {
				milestoneTitle?.focus();
			} else {
				const card = document.querySelector('.review-session');
				if (card && card.getBoundingClientRect().top < 90) card.scrollIntoView({ block: 'start' });
			}
		} catch {
			error = 'That choice could not be saved. Try again.';
		} finally {
			submitting = false;
		}
	}
</script>

<svelte:head>
	<title>Review Highlights · BookReplay</title>
	<meta name="description" content="Revisit and schedule your Kindle highlights" />
</svelte:head>

<a class="absolute top-3 left-3 z-50 -translate-y-[200%] rounded-md bg-ink px-3 py-2 text-cream focus-visible:translate-y-0" href="#main-content">Skip to content</a>

<main id="main-content" aria-busy={loading || submitting} class="review-main">
	<div class="review-topline"><a class="quiet-link" href="/">← Your daily practice</a><span class="text-sm text-muted">A little space to reflect</span></div>
	{#if loading}
		<p class="empty-state" role="status">Preparing a few good ideas…</p>
	{:else if error && highlights.length === 0}
		<p class="empty-state text-danger" role="alert">{error}</p>
	{:else if goalReached || complete}
		<section class="completion-card page-enter" aria-labelledby="complete-title">
			<div class="completion-seal streak-burst" aria-hidden="true">✦</div>
			<p class="eyebrow">{goalReached ? 'Five little moments, well spent' : 'A little wiser than before'}</p>
			<h1 id="complete-title" bind:this={milestoneTitle} tabindex="-1">{goalReached ? 'You showed up for yourself.' : 'Let those ideas settle.'}</h1>
			<p class="intro-copy">{goalReached ? 'Your daily ritual is complete. Take a thought you love into the rest of your day.' : `You revisited ${currentIndex} highlight${currentIndex === 1 ? '' : 's'} this session. Each visit helps the good ideas stay.`}</p>
			{#if progressLoaded}<DailyProgress count={dailyRevisionCount} {streak} />{/if}
			<div class="mt-8 flex flex-wrap justify-center gap-3">
				<a class="primary-button" href="/">Done for now <span aria-hidden="true">✓</span></a>
				{#if current}<button class="secondary-button" type="button" onclick={() => (goalReached = false)}>Revisit a few more →</button>{/if}
			</div>
			<p class="mt-5 text-sm text-muted">{totals.ARCHIVE ? `${totals.ARCHIVE} archived · ` : ''}{currentIndex} reviewed this visit</p>
		</section>
	{:else if !current}
		<section class="completion-card page-enter" aria-labelledby="caught-up-title">
			<div class="completion-seal" aria-hidden="true">✓</div>
			<p class="eyebrow">Room to let things sink in</p>
			<h1 id="caught-up-title">You’re all caught up.</h1>
			<p class="intro-copy">Your next ideas will return when they’re ready. In the meantime, there’s a whole bookshelf to wander through.</p>
			<a class="primary-button" href="/?view=books">Visit your bookshelf →</a>
		</section>
	{:else}
		{#if progressLoaded}<DailyProgress count={dailyRevisionCount} {streak} />{/if}
		<section class="review-session" aria-labelledby="review-title">
			<div class="flex items-center justify-between gap-4 mb-4">
				<p class="eyebrow">One idea at a time</p>
				<p class="text-xs text-muted">{currentIndex + 1} of {highlights.length} in this visit</p>
			</div>
			{#key current.highlight_id}
				<article class="highlight-card page-enter">
					<span class="card-bookmark" aria-hidden="true"></span>
					<header>
						<h1 id="review-title">{current.book_title}</h1>
						<p class="mt-2 text-sm text-muted">{current.authors.length ? current.authors.join(', ') : 'Unknown author'}</p>
					</header>
					<span class="quote-mark" aria-hidden="true">“</span>
					<blockquote>{current.highlight_text}</blockquote>
					<p class="reflection-prompt">Pause for a moment. What stands out to you now?</p>
				</article>
			{/key}
			<div class="review-choices">
				<h2 class="font-serif text-xl">When would you like to revisit this?</h2>
				<p class="mt-1 text-sm text-muted">There’s no right answer. Choose what feels useful.</p>
				<div class="rating-grid">
					<button type="button" disabled={submitting} onclick={() => review('SOON')}><span>Soon</span><small>Keep it close</small></button>
					<button type="button" disabled={submitting} onclick={() => review('LATER')}><span>Later</span><small>Let it settle</small></button>
					<button type="button" disabled={submitting} onclick={() => review('MUCH_LATER')}><span>Much later</span><small>Give it space</small></button>
				</div>
				<button class="archive-button" type="button" disabled={submitting} onclick={() => review('ARCHIVE')}>Archive · Keep in library, stop reviews</button>
				{#if error}<p class="mt-3 text-danger" role="alert">{error}</p>{/if}
			</div>
		</section>
	{/if}
	<p class="save-feedback" role="status" aria-live="polite">{submitting ? 'Saving your choice…' : feedback}</p>
</main>
