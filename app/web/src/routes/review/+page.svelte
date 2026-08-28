<script lang="ts">
	import { onMount } from 'svelte';
	import LogoutButton from '$lib/LogoutButton.svelte';

	type Rating = 'SOON' | 'LATER' | 'MUCH_LATER' | 'ARCHIVE';

	type Highlight = {
		highlight_id: number;
		book_title: string;
		authors: string[];
		highlight_text: string;
	};

	let highlights = $state<Highlight[]>([]);
	let currentIndex = $state(0);
	let loading = $state(true);
	let submitting = $state(false);
	let error = $state('');
	let totals = $state<Record<Rating, number>>({
		SOON: 0,
		LATER: 0,
		MUCH_LATER: 0,
		ARCHIVE: 0
	});
	let current = $derived(highlights[currentIndex]);
	let complete = $derived(highlights.length > 0 && currentIndex === highlights.length);

	onMount(async () => {
		try {
			const response = await fetch('/api/reviews/highlights/session');
			if (!response.ok) throw new Error();
			highlights = (await response.json()) as Highlight[];
		} catch {
			error = 'Could not load your review session. Try refreshing the page.';
		} finally {
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

			totals = { ...totals, [rating]: totals[rating] + 1 };
			currentIndex += 1;
		} catch {
			error = 'That choice could not be saved. Try again.';
		} finally {
			submitting = false;
		}
	}
</script>

<svelte:head>
	<title>Review Highlights · Rekindle</title>
	<meta name="description" content="Revisit and schedule your Kindle highlights" />
</svelte:head>

<a class="absolute top-3 left-3 z-10 -translate-y-[200%] rounded-md bg-ink px-3 py-2 text-cream focus-visible:translate-y-0" href="#main-content">Skip to content</a>

<main id="main-content" aria-busy={loading || submitting} class="mx-auto min-h-screen w-[calc(100%-2rem)] max-w-4xl pt-5 pb-20 max-sm:w-[calc(100%-1.5rem)]">
	<nav class="flex items-center justify-between gap-4" aria-label="Primary navigation">
		<a class="font-serif text-[1.4rem] font-bold text-ink no-underline focus-visible:outline-3 focus-visible:outline-offset-4 focus-visible:outline-clay" href="/">Rekindle</a>
		<div class="flex items-center gap-4">
			<a class="text-sm font-bold text-forest focus-visible:outline-3 focus-visible:outline-offset-4 focus-visible:outline-clay" href="/">← Your bookshelf</a>
			<LogoutButton />
		</div>
	</nav>

	{#if loading}
		<p class="mt-[35vh] text-center text-muted" role="status" aria-live="polite">Preparing your highlights…</p>
	{:else if error && highlights.length === 0}
		<p class="mt-[35vh] text-center text-danger" role="alert">{error}</p>
	{:else if complete}
		<section class="mx-auto mt-[clamp(5rem,15vw,10rem)] max-w-xl text-center" aria-labelledby="complete-title">
			<p class="mb-4 text-3xl text-clay" aria-hidden="true">✦</p>
			<h1 id="complete-title" class="mb-3 font-serif text-[clamp(2.5rem,8vw,4.5rem)] leading-none tracking-[-0.03em]">Review complete</h1>
			<p class="mb-8 text-lg text-muted">{highlights.length} highlight{highlights.length === 1 ? '' : 's'} reviewed</p>
			<dl class="mx-auto grid max-w-md grid-cols-2 gap-3 text-left">
				<div class="rounded-lg border border-line bg-cream p-4"><dt class="text-sm text-muted">Soon</dt><dd class="mt-1 text-2xl font-bold">{totals.SOON}</dd></div>
				<div class="rounded-lg border border-line bg-cream p-4"><dt class="text-sm text-muted">Later</dt><dd class="mt-1 text-2xl font-bold">{totals.LATER}</dd></div>
				<div class="rounded-lg border border-line bg-cream p-4"><dt class="text-sm text-muted">Much later</dt><dd class="mt-1 text-2xl font-bold">{totals.MUCH_LATER}</dd></div>
				<div class="rounded-lg border border-line bg-cream p-4"><dt class="text-sm text-muted">Archived</dt><dd class="mt-1 text-2xl font-bold">{totals.ARCHIVE}</dd></div>
			</dl>
		</section>
	{:else if !current}
		<section class="mx-auto mt-[clamp(5rem,15vw,10rem)] max-w-xl text-center" aria-labelledby="caught-up-title">
			<p class="mb-4 text-3xl text-clay" aria-hidden="true">✦</p>
			<h1 id="caught-up-title" class="mb-3 font-serif text-[clamp(2.5rem,8vw,4.5rem)] leading-none tracking-[-0.03em]">You’re all caught up</h1>
			<p class="text-lg text-muted">There are no highlights ready to review right now.</p>
		</section>
	{:else}
		<section class="mx-auto mt-[clamp(3rem,9vw,6rem)] max-w-3xl" aria-labelledby="review-title">
			<div class="mb-4 flex items-center justify-between gap-4">
				<p class="m-0 text-xs font-extrabold tracking-[0.14em] text-clay uppercase">Highlight review</p>
				<p class="m-0 text-sm font-bold text-muted" aria-label={`Highlight ${currentIndex + 1} of ${highlights.length}`}>{currentIndex + 1} / {highlights.length}</p>
			</div>

			<article class="rounded-xl border border-line bg-cream p-[clamp(1.5rem,6vw,3.5rem)] shadow-[0_1.25rem_3rem_rgb(80_65_40/10%)]">
				<header class="border-b border-line pb-6">
					<h1 id="review-title" class="m-0 font-serif text-[clamp(1.65rem,5vw,2.5rem)] leading-tight text-balance">{current.book_title}</h1>
					<p class="mt-2 mb-0 text-muted">{current.authors.length ? current.authors.join(', ') : 'Unknown author'}</p>
				</header>
				<blockquote class="my-[clamp(2rem,7vw,4rem)] font-serif text-[clamp(1.25rem,3.5vw,1.65rem)] leading-[1.65] break-words">{current.highlight_text}</blockquote>
			</article>

			<div class="mt-8 text-center">
				<h2 class="mb-5 font-serif text-[clamp(1.35rem,4vw,1.8rem)]">When would you like to see this idea again?</h2>
				<div class="grid grid-cols-3 gap-3 max-sm:grid-cols-1">
					<button class="cursor-pointer rounded-md border border-forest bg-forest px-4 py-3.5 font-[inherit] font-bold text-cream hover:bg-forest-dark focus-visible:outline-3 focus-visible:outline-offset-3 focus-visible:outline-clay disabled:cursor-wait disabled:opacity-60" type="button" disabled={submitting} onclick={() => review('SOON')}>Soon <span class="block text-xs font-normal opacity-80">About 3 days</span></button>
					<button class="cursor-pointer rounded-md border border-forest bg-forest px-4 py-3.5 font-[inherit] font-bold text-cream hover:bg-forest-dark focus-visible:outline-3 focus-visible:outline-offset-3 focus-visible:outline-clay disabled:cursor-wait disabled:opacity-60" type="button" disabled={submitting} onclick={() => review('LATER')}>Later <span class="block text-xs font-normal opacity-80">About 2 weeks</span></button>
					<button class="cursor-pointer rounded-md border border-forest bg-forest px-4 py-3.5 font-[inherit] font-bold text-cream hover:bg-forest-dark focus-visible:outline-3 focus-visible:outline-offset-3 focus-visible:outline-clay disabled:cursor-wait disabled:opacity-60" type="button" disabled={submitting} onclick={() => review('MUCH_LATER')}>Much later <span class="block text-xs font-normal opacity-80">About 2 months</span></button>
				</div>
				<button class="mt-5 cursor-pointer border-0 bg-transparent p-2 font-[inherit] text-sm font-bold text-muted underline decoration-line underline-offset-4 hover:text-danger focus-visible:outline-3 focus-visible:outline-offset-3 focus-visible:outline-clay disabled:cursor-wait disabled:opacity-60" type="button" disabled={submitting} onclick={() => review('ARCHIVE')}>Archive this highlight</button>
				{#if error}<p class="mt-4 text-danger" role="alert" aria-live="assertive">{error}</p>{/if}
			</div>
		</section>
	{/if}
</main>
