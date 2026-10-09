<script lang="ts">
	import { onMount } from 'svelte';
	import BookDetail from '$lib/BookDetail.svelte';
	import BookGrid from '$lib/BookGrid.svelte';
	import DailyProgress from '$lib/DailyProgress.svelte';
	import IdentifyBookDialog from '$lib/IdentifyBookDialog.svelte';
	import { browser } from '$app/environment';
	import { page } from '$app/state';
	import { loadProgress, progress } from '$lib/progress.svelte';
	import type { BookSummary } from '$lib/types';

	let books = $state<BookSummary[]>([]);
	let libraryOpen = $state(false);
	let loading = $state(true);
	let error = $state('');
	let identifyDialog: IdentifyBookDialog;
	let booksView = $derived(browser && page.url.searchParams.get('view') === 'books');
	// The open book lives in the URL, so Back returns to the shelf and a book can be linked.
	let selectedBookId = $derived(browser ? Number(page.url.searchParams.get('book')) || null : null);
	let selectedBook = $derived(books.find((book) => book.id === selectedBookId));

	onMount(async () => {
		const loadBooks = fetch('/api/books')
			.then(async (response) => {
				if (!response.ok) throw new Error();
				books = (await response.json()) as BookSummary[];
			})
			.catch(() => (error = 'Could not load your library. Try refreshing the page.'));
		await Promise.all([loadBooks, loadProgress()]);
		loading = false;
	});
</script>

<svelte:head>
	<title>{selectedBook?.title ?? (booksView ? 'Books' : 'Revision')} · BookReplay</title>
	<meta name="description" content="Revisit the ideas worth keeping" />
</svelte:head>

<a class="absolute top-3 left-3 z-50 -translate-y-[200%] rounded-md bg-ink px-3 py-2 text-cream focus-visible:translate-y-0" href="#main-content">Skip to content</a>

<main id="main-content" aria-busy={loading} class="mx-auto min-h-screen w-[calc(100%-2rem)] max-w-6xl pt-5 pb-20 max-sm:w-[calc(100%-1.5rem)]">
	{#if loading}
		<p class="mt-[30vh] text-center text-muted" role="status" aria-live="polite">Loading your bookshelf…</p>
	{:else if selectedBook}
		<BookDetail book={selectedBook} />
	{:else}
		{#if !booksView}
		<section class="ritual-home page-enter" aria-labelledby="revision-title">
			<div class="ritual-intro">
				<p class="eyebrow">Your daily pause</p>
				<h1 id="revision-title">Good ideas deserve<br /><em>another visit.</em></h1>
				<p class="intro-copy">Settle in. Revisit a few words you loved, and take something with you into today.</p>
				{#if !progress.loaded}
					<p class="text-danger" role="alert">Could not load today’s revision queue.</p>
				{:else if progress.dueCount}
					<a class="primary-button" href="/review/">{progress.dailyRevisionCount >= 5 ? 'Revisit a few more' : progress.dailyRevisionCount > 0 ? 'Continue your ritual' : 'Begin today’s ritual'} <span aria-hidden="true">→</span></a>
					<p class="mt-4 text-sm text-muted">{progress.dueCount} idea{progress.dueCount === 1 ? '' : 's'} ready to revisit · At your own pace</p>
				{:else if books.length}
					<p class="mb-5 text-muted">You’re all caught up. Your ideas will be here when they’re ready.</p>
					<a class="secondary-button" href="/?view=books">Spend time with your books <span aria-hidden="true">→</span></a>
				{:else}
					<a class="primary-button" href="/import/">Bring in your first highlights <span aria-hidden="true">→</span></a>
					<p class="mt-4 text-sm text-muted">Start with your Kindle clippings.</p>
				{/if}
			</div>
			<div class="ritual-aside">
				<div class="reading-mark" aria-hidden="true"><svg viewBox="0 0 180 110" fill="none"><path d="M20 85Q53 69 90 87Q127 69 160 85L153 34Q119 21 90 40Q61 21 27 34Z" fill="var(--color-sage)" stroke="var(--color-link)" stroke-width="2"/><path d="M90 40V87M36 46Q60 40 79 50M35 58Q60 52 79 62M101 50Q123 40 145 46M101 62Q124 52 146 58" stroke="var(--color-link)" stroke-width="2" stroke-linecap="round"/><path d="M89 24V12M73 27L68 19M105 27L111 19" stroke="var(--color-clay)" stroke-width="2" stroke-linecap="round"/></svg></div>
				{#if progress.loaded}<DailyProgress count={progress.dailyRevisionCount} streak={progress.streak} />{:else}<p class="text-sm text-muted">Your daily progress is unavailable. You can still review.</p>{/if}
				<p class="mt-4 text-center text-sm italic text-muted">Small moments. Lasting ideas.</p>
			</div>
		</section>
		{/if}

		<section class={booksView ? 'mx-auto mt-9 max-w-4xl' : 'mx-auto mt-12 max-w-5xl border-t border-line pt-7'} aria-labelledby="library-title">
			<div class="flex flex-wrap items-end justify-between gap-4">
				<div>
					{#if !booksView}<p class="mb-1 text-xs font-extrabold tracking-[0.14em] text-clay uppercase">The words you’ve kept</p>{/if}
					<h2 id="library-title" class="font-serif text-3xl tracking-[-0.03em]">Your library</h2>
					{#if !booksView}<p class="mt-1 text-muted">Browse or edit highlights when you need them.</p>{/if}
				</div>
				{#if books.length && !booksView}
					<button class="cursor-pointer rounded-full border border-input-border bg-transparent px-4 py-2.5 font-[inherit] font-bold text-link hover:border-forest hover:bg-sage focus-visible:outline-3 focus-visible:outline-offset-4 focus-visible:outline-clay" type="button" onclick={() => (libraryOpen = !libraryOpen)}>{libraryOpen ? 'Hide books' : `Browse ${books.length} book${books.length === 1 ? '' : 's'}`}</button>
				{/if}
			</div>
			{#if error}
				<p class="mt-5 text-danger" role="alert">{error}</p>
			{:else if !books.length}
				<p class="mt-5 text-muted">Import your Kindle clippings to build your library.</p>
			{/if}
		</section>

		{#if (booksView || libraryOpen) && books.length}
			<BookGrid {books} compact={booksView} onidentify={(book) => identifyDialog.open(book)} />
		{/if}
	{/if}
</main>

<IdentifyBookDialog
	bind:this={identifyDialog}
	onidentified={(updated) => (books = books.map((book) => (book.id === updated.id ? updated : book)))}
/>
