<script lang="ts">
	import { onMount } from 'svelte';

	type Book = {
		id: number;
		title: string;
		authors: string[];
		cover_url: string | null;
	};

	type BookSummary = Book & {
		highlight_count: number;
	};

	type Clipping = {
		book: Book;
		metadata: string;
		content: string;
	};

	let books = $state<BookSummary[]>([]);
	let highlights = $state<Clipping[]>([]);
	let selectedBookId = $state<number | null>(null);
	let loading = $state(true);
	let loadingHighlights = $state(false);
	let error = $state('');
	let highlightsError = $state('');
	let selectedBook = $derived(books.find((book) => book.id === selectedBookId));

	onMount(async () => {
		try {
			const response = await fetch('/api/books');
			if (!response.ok) throw new Error();
			books = (await response.json()) as BookSummary[];
		} catch {
			error = 'Could not load your bookshelf. Try refreshing the page.';
		} finally {
			loading = false;
		}
	});

	async function selectBook(bookId: number) {
		selectedBookId = bookId;
		highlights = [];
		highlightsError = '';
		loadingHighlights = true;

		try {
			const response = await fetch('/api/clippings');
			if (!response.ok) throw new Error();

			highlights = ((await response.json()) as Clipping[]).filter(
				(clipping) => clipping.book.id === bookId && clipping.content.trim()
			);
		} catch {
			highlightsError = 'Could not load this book’s highlights. Try again.';
		} finally {
			loadingHighlights = false;
		}
	}
</script>

<svelte:head>
	<title>Bookshelf · Rekindle</title>
	<meta name="description" content="Browse Kindle highlights by book" />
</svelte:head>

<a class="absolute top-3 left-3 z-10 -translate-y-[200%] rounded-md bg-ink px-3 py-2 text-cream focus-visible:translate-y-0" href="#main-content">Skip to content</a>

<main id="main-content" aria-busy={loading} class="mx-auto min-h-screen w-[calc(100%-2rem)] max-w-6xl pt-5 pb-20 max-sm:w-[calc(100%-1.5rem)]">
	<nav class="flex items-center justify-between gap-4" aria-label="Primary navigation">
		<a class="font-serif text-[1.4rem] font-bold text-ink no-underline focus-visible:outline-3 focus-visible:outline-offset-4 focus-visible:outline-clay" href="/" aria-current="page">Rekindle</a>
		<a class="rounded-full border border-[#aeb9a6] px-3.5 py-2 text-sm font-bold text-forest no-underline hover:border-forest hover:bg-sage focus-visible:border-forest focus-visible:bg-sage focus-visible:outline-3 focus-visible:outline-offset-4 focus-visible:outline-clay" href="/import/">Import clippings <span aria-hidden="true">↗</span></a>
	</nav>

	{#if loading}
		<p class="mt-[30vh] text-center text-muted" role="status" aria-live="polite">Loading your bookshelf…</p>
	{:else if error}
		<p class="mt-[30vh] text-center text-danger" role="alert" aria-live="polite">{error}</p>
	{:else if selectedBook}
		<button class="mt-12 mb-6 cursor-pointer border-0 bg-transparent p-0 font-[inherit] font-bold text-forest hover:text-clay focus-visible:outline-3 focus-visible:outline-offset-4 focus-visible:outline-clay" type="button" onclick={() => (selectedBookId = null)}>← All books</button>
		<header class="flex items-end gap-[clamp(1.25rem,4vw,2.5rem)] border-b border-line pb-10 max-sm:items-start">
			{#if selectedBook.cover_url}
				<img
					class="h-60 w-40 shrink-0 rounded-sm object-cover shadow-cover max-sm:h-36 max-sm:w-24"
					src={selectedBook.cover_url}
					alt={`Cover of ${selectedBook.title}`}
					width="160"
					height="240"
				/>
			{:else}
				<div class="grid h-60 w-40 shrink-0 place-items-center rounded-sm bg-forest font-serif text-[clamp(2.5rem,6vw,4rem)] text-cream shadow-cover max-sm:h-36 max-sm:w-24" aria-hidden="true">{selectedBook.title.slice(0, 1)}</div>
			{/if}
			<div class="min-w-0">
				<p class="mb-2.5 text-xs font-extrabold tracking-[0.14em] text-clay uppercase">Selected book</p>
				<h1 class="mb-3 max-w-[38rem] font-serif text-[clamp(2.25rem,6vw,4.5rem)] leading-[0.98] tracking-[-0.04em] text-balance break-words">{selectedBook.title}</h1>
				{#if selectedBook.authors.length}
					<p class="mb-0 text-[1.05rem] text-muted">{selectedBook.authors.join(', ')}</p>
				{/if}
				<span class="mt-2.5 inline-block rounded-full bg-chip px-2 py-1 text-xs leading-tight font-bold text-chip-text">{selectedBook.highlight_count} highlight{selectedBook.highlight_count === 1 ? '' : 's'}</span>
			</div>
		</header>

		{#if loadingHighlights}
			<p class="mt-[30vh] text-center text-muted" role="status" aria-live="polite">Loading highlights…</p>
		{:else if highlightsError}
			<p class="mt-[30vh] text-center text-danger" role="alert" aria-live="polite">{highlightsError}</p>
		{:else}
			<section class="mx-auto mt-10 grid w-full max-w-3xl gap-4" aria-label={`Highlights from ${selectedBook.title}`}>
				{#each highlights as highlight}
					<article class="rounded-lg border border-line bg-cream p-[clamp(1.25rem,4vw,2rem)] shadow-[0_0.4rem_1.3rem_rgb(80_65_40/6%)]">
						<blockquote class="mb-5 font-serif text-[clamp(1.05rem,2vw,1.2rem)] leading-[1.7] break-words">{highlight.content}</blockquote>
						<footer class="text-xs text-muted-light break-words">{highlight.metadata}</footer>
					</article>
				{/each}
			</section>
		{/if}
	{:else}
		<header class="mt-[clamp(4rem,11vw,8rem)] mb-12 max-w-2xl">
			<p class="mb-2.5 text-xs font-extrabold tracking-[0.14em] text-clay uppercase">Your library</p>
			<h1 class="mb-3 font-serif text-[clamp(2.5rem,8vw,5.2rem)] leading-[0.98] tracking-[-0.04em] text-balance">Your Bookshelf</h1>
			<p class="mb-0 text-[1.05rem] text-muted">Choose a book to revisit what stood out.</p>
		</header>

		{#if books.length}
			<section class="grid grid-cols-[repeat(auto-fill,minmax(min(100%,11rem),1fr))] gap-x-[clamp(0.9rem,2.5vw,1.5rem)] gap-y-[clamp(1.25rem,3vw,2rem)]" aria-label="Books with highlights">
				{#each books as book}
					<button class="group min-w-0 cursor-pointer border border-transparent bg-transparent p-0 text-left font-[inherit] text-inherit focus-visible:outline-3 focus-visible:outline-offset-4 focus-visible:outline-clay" type="button" onclick={() => selectBook(book.id)}>
						{#if book.cover_url}
							<img
								class="mb-3.5 block aspect-2/3 h-auto w-full rounded-sm object-cover shadow-cover transition duration-150 group-hover:-translate-y-1 group-hover:shadow-cover-hover group-focus-visible:-translate-y-1 group-focus-visible:shadow-cover-hover motion-reduce:transition-none"
								src={book.cover_url}
								alt=""
								width="240"
								height="360"
								loading="lazy"
							/>
						{:else}
							<div class="mb-3.5 grid aspect-2/3 w-full place-items-center rounded-sm bg-forest font-serif text-[clamp(2.5rem,6vw,4rem)] text-cream shadow-cover transition duration-150 group-hover:-translate-y-1 group-hover:shadow-cover-hover group-focus-visible:-translate-y-1 group-focus-visible:shadow-cover-hover motion-reduce:transition-none" aria-hidden="true">{book.title.slice(0, 1)}</div>
						{/if}
						<span class="line-clamp-2 min-w-0 font-bold leading-[1.3] break-words">{book.title}</span>
						{#if book.authors.length}<span class="mt-1 line-clamp-1 min-w-0 text-sm leading-[1.3] break-words text-muted">{book.authors.join(', ')}</span>{/if}
						<span class="mt-2.5 inline-block rounded-full bg-chip px-2 py-1 text-xs leading-tight font-bold text-chip-text">{book.highlight_count} highlight{book.highlight_count === 1 ? '' : 's'}</span>
					</button>
				{/each}
			</section>
		{:else}
			<section class="mx-auto mt-32 max-w-lg text-center" aria-label="Empty bookshelf">
				<p class="mb-4 text-3xl text-clay" aria-hidden="true">✦</p>
				<h2 class="mb-2 font-serif text-3xl text-balance">Your shelf is waiting</h2>
				<p class="text-muted">Import your Kindle clippings to make this space your own.</p>
				<a class="mt-3 inline-block rounded-full border border-[#aeb9a6] px-4 py-3 font-bold text-forest no-underline hover:border-forest hover:bg-sage focus-visible:border-forest focus-visible:bg-sage focus-visible:outline-3 focus-visible:outline-offset-4 focus-visible:outline-clay" href="/import/">Import clippings</a>
			</section>
		{/if}
	{/if}
</main>
