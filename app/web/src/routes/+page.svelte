<script lang="ts">
	import { onMount, tick } from 'svelte';
	import LogoutButton from '$lib/LogoutButton.svelte';

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
		id: number;
		book: Book;
		metadata: string;
		content: string;
	};

	type ReviewHighlight = {
		highlight_id: number;
	};

	type Streak = {
		streak: number;
		daily_revision_count: number;
	};

	type OpenLibraryBook = {
		open_library_key: string;
		title: string;
		authors: string[];
		cover_id: number | null;
		first_publish_year: number | null;
		edition_count: number | null;
		isbns: string[];
	};

	let books = $state<BookSummary[]>([]);
	let reviewHighlights = $state<ReviewHighlight[]>([]);
	let streak = $state(0);
	let dailyRevisionCount = $state(0);
	let highlights = $state<Clipping[]>([]);
	let selectedBookId = $state<number | null>(null);
	let libraryOpen = $state(false);
	let loading = $state(true);
	let loadingHighlights = $state(false);
	let error = $state('');
	let reviewError = $state('');
	let highlightsError = $state('');
	let editingHighlight = $state<Clipping | null>(null);
	let editedContent = $state('');
	let savingHighlight = $state(false);
	let editError = $state('');
	let identificationDialog: HTMLDialogElement;
	let searchInput: HTMLInputElement;
	let identifyingBook = $state<BookSummary | null>(null);
	let searchQuery = $state('');
	let searchResults = $state<OpenLibraryBook[]>([]);
	let searching = $state(false);
	let saving = $state(false);
	let identificationError = $state('');
	let searchRequest = 0;
	let selectedBook = $derived(books.find((book) => book.id === selectedBookId));

	onMount(async () => {
		const loadBooks = fetch('/api/books')
			.then(async (response) => {
				if (!response.ok) throw new Error();
				books = (await response.json()) as BookSummary[];
			})
			.catch(() => (error = 'Could not load your library. Try refreshing the page.'));
		const loadReviewQueue = fetch('/api/reviews/highlights/session')
			.then(async (response) => {
				if (!response.ok) throw new Error();
				reviewHighlights = (await response.json()) as ReviewHighlight[];
			})
			.catch(() => (reviewError = 'Could not load today’s revision queue.'));
		const loadStreak = fetch('/api/reviews/streak')
			.then(async (response) => {
				if (!response.ok) throw new Error();
				const progress = (await response.json()) as Streak;
				streak = progress.streak;
				dailyRevisionCount = progress.daily_revision_count;
			})
			.catch(() => {});

		await Promise.all([loadBooks, loadReviewQueue, loadStreak]);
		loading = false;
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

	function editHighlight(highlight: Clipping) {
		editingHighlight = highlight;
		editedContent = highlight.content;
		editError = '';
	}

	async function saveHighlight() {
		if (!editingHighlight || savingHighlight) return;

		savingHighlight = true;
		editError = '';
		try {
			const response = await fetch(`/api/clippings/${editingHighlight.id}`, {
				method: 'PATCH',
				headers: { 'content-type': 'application/json' },
				body: JSON.stringify({ content: editedContent })
			});
			if (!response.ok) throw new Error();

			const { content } = (await response.json()) as { content: string };
			highlights = highlights.map((highlight) =>
				highlight.id === editingHighlight?.id ? { ...highlight, content } : highlight
			);
			editingHighlight = null;
		} catch {
			editError = 'Could not save this highlight. Try again.';
		} finally {
			savingHighlight = false;
		}
	}

	async function openIdentification(book: BookSummary) {
		identifyingBook = book;
		searchQuery = book.title;
		searchResults = [];
		identificationError = '';
		await tick();
		identificationDialog.showModal();
		searchInput.focus();
		await searchBooks();
	}

	async function searchBooks() {
		const query = searchQuery.trim();
		if (!query) return;

		const request = ++searchRequest;
		searching = true;
		identificationError = '';
		searchResults = [];
		try {
			const response = await fetch(`/api/books/search?q=${encodeURIComponent(query)}`);
			const body = await response.json();
			if (!response.ok) throw new Error(body.error);
			if (request === searchRequest) searchResults = body as OpenLibraryBook[];
		} catch {
			if (request === searchRequest) identificationError = 'Could not search Open Library. Try again.';
		} finally {
			if (request === searchRequest) searching = false;
		}
	}

	async function identifyBook(candidate: OpenLibraryBook) {
		if (!identifyingBook) return;

		saving = true;
		identificationError = '';
		try {
			const response = await fetch(`/api/books/${identifyingBook.id}/identification`, {
				method: 'PUT',
				headers: { 'content-type': 'application/json' },
				body: JSON.stringify(candidate)
			});
			const body = await response.json();
			if (!response.ok) throw new Error(body.error);

			const updated = body as BookSummary;
			books = books.map((book) => (book.id === updated.id ? updated : book));
			identificationDialog.close();
		} catch {
			identificationError = 'Could not identify this book. Try again.';
		} finally {
			saving = false;
		}
	}
</script>

<svelte:head>
	<title>Revision · BookReplay</title>
	<meta name="description" content="Revisit the ideas worth keeping" />
</svelte:head>

<a class="absolute top-3 left-3 z-10 -translate-y-[200%] rounded-md bg-ink px-3 py-2 text-cream focus-visible:translate-y-0" href="#main-content">Skip to content</a>

<main id="main-content" aria-busy={loading} class="mx-auto min-h-screen w-[calc(100%-2rem)] max-w-6xl pt-5 pb-20 max-sm:w-[calc(100%-1.5rem)]">
	<nav class="flex items-center justify-between gap-4" aria-label="Primary navigation">
		<a class="font-serif text-[1.4rem] font-bold text-ink no-underline focus-visible:outline-3 focus-visible:outline-offset-4 focus-visible:outline-clay" href="/" aria-current="page">BookReplay</a>
		<div class="flex items-center gap-3">
			<a class="text-sm font-bold text-forest focus-visible:outline-3 focus-visible:outline-offset-4 focus-visible:outline-clay" href="/review/">Revise now</a>
			<a class="rounded-full border border-[#aeb9a6] px-3.5 py-2 text-sm font-bold text-forest no-underline hover:border-forest hover:bg-sage focus-visible:border-forest focus-visible:bg-sage focus-visible:outline-3 focus-visible:outline-offset-4 focus-visible:outline-clay" href="/import/">Import clippings <span aria-hidden="true">↗</span></a>
			<LogoutButton />
		</div>
	</nav>

	{#if loading}
		<p class="mt-[30vh] text-center text-muted" role="status" aria-live="polite">Loading your bookshelf…</p>
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
						<div class="mb-5 flex items-start justify-between gap-4">
							<blockquote class="m-0 font-serif text-[clamp(1.05rem,2vw,1.2rem)] leading-[1.7] break-words">{highlight.content}</blockquote>
							<button class="shrink-0 cursor-pointer rounded-md border border-[#aeb9a6] bg-transparent px-3 py-1.5 font-[inherit] text-sm font-bold text-forest hover:border-forest hover:bg-sage focus-visible:outline-3 focus-visible:outline-offset-2 focus-visible:outline-clay" type="button" onclick={() => editHighlight(highlight)}>Edit</button>
						</div>
						<footer class="text-xs text-muted-light break-words">{highlight.metadata}</footer>
					</article>
				{/each}
			</section>
		{/if}
	{:else}
		<section class="mx-auto mt-[clamp(4rem,11vw,8rem)] max-w-3xl text-center" aria-labelledby="revision-title">
			<p class="mb-3 text-xs font-extrabold tracking-[0.14em] text-clay uppercase">Your revision practice</p>
			<h1 id="revision-title" class="mb-4 font-serif text-[clamp(2.75rem,9vw,5.8rem)] leading-[0.94] tracking-[-0.05em] text-balance">Return to what matters.</h1>
			{#if streak}
				<p class="mb-4 text-sm font-bold text-forest">✦ {streak}-day revision streak</p>
			{/if}
			<p class="mb-4 text-sm text-muted">{dailyRevisionCount} / 5 revisions today</p>
			{#if reviewError}
				<p class="text-danger" role="alert">{reviewError}</p>
			{:else if reviewHighlights.length}
				<p class="mx-auto mb-7 max-w-xl text-[1.1rem] leading-relaxed text-muted">{reviewHighlights.length} idea{reviewHighlights.length === 1 ? '' : 's'} are ready for a fresh look. A few minutes now keeps the useful parts close.</p>
				<a class="inline-block rounded-full border border-forest bg-forest px-6 py-3.5 font-bold text-cream no-underline shadow-cover hover:bg-forest-dark focus-visible:outline-3 focus-visible:outline-offset-4 focus-visible:outline-clay" href="/review/">Start today’s revision <span aria-hidden="true">→</span></a>
			{:else}
				<p class="mx-auto mb-7 max-w-xl text-[1.1rem] leading-relaxed text-muted">You’re up to date. Your next ideas will return when they’re ready.</p>
				<a class="inline-block rounded-full border border-[#aeb9a6] px-6 py-3.5 font-bold text-forest no-underline hover:border-forest hover:bg-sage focus-visible:border-forest focus-visible:bg-sage focus-visible:outline-3 focus-visible:outline-offset-4 focus-visible:outline-clay" href="/import/">Import more highlights</a>
			{/if}
		</section>

		<section class="mx-auto mt-[clamp(5rem,12vw,9rem)] max-w-4xl border-t border-line pt-7" aria-labelledby="library-title">
			<div class="flex flex-wrap items-end justify-between gap-4">
				<div>
					<p class="mb-1 text-xs font-extrabold tracking-[0.14em] text-clay uppercase">Reference</p>
					<h2 id="library-title" class="font-serif text-3xl tracking-[-0.03em]">Your library</h2>
					<p class="mt-1 text-muted">Browse or edit highlights when you need them.</p>
				</div>
				{#if books.length}
					<button class="cursor-pointer rounded-full border border-[#aeb9a6] bg-transparent px-4 py-2.5 font-[inherit] font-bold text-forest hover:border-forest hover:bg-sage focus-visible:outline-3 focus-visible:outline-offset-4 focus-visible:outline-clay" type="button" onclick={() => (libraryOpen = !libraryOpen)}>{libraryOpen ? 'Hide books' : `Browse ${books.length} book${books.length === 1 ? '' : 's'}`}</button>
				{/if}
			</div>
			{#if error}
				<p class="mt-5 text-danger" role="alert">{error}</p>
			{:else if !books.length}
				<p class="mt-5 text-muted">Import your Kindle clippings to build your library.</p>
			{/if}
		</section>

		{#if libraryOpen && books.length}
			<section class="mt-8 grid grid-cols-[repeat(auto-fill,minmax(min(100%,10rem),1fr))] gap-x-[clamp(0.9rem,2.5vw,1.5rem)] gap-y-[clamp(1.25rem,3vw,2rem)]" aria-label="Books with highlights">
				{#each books as book}
					<article class="relative min-w-0">
						<button class="group block w-full min-w-0 cursor-pointer border border-transparent bg-transparent p-0 text-left font-[inherit] text-inherit focus-visible:outline-3 focus-visible:outline-offset-4 focus-visible:outline-clay" type="button" onclick={() => selectBook(book.id)}>
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
						<details class="absolute top-2 right-2">
							<summary class="grid h-10 w-10 cursor-pointer list-none place-items-center rounded-full border border-line bg-cream/95 text-xl leading-none font-bold shadow-sm hover:bg-sage focus-visible:outline-3 focus-visible:outline-offset-2 focus-visible:outline-clay [&::-webkit-details-marker]:hidden" aria-label={`Actions for ${book.title}`}>•••</summary>
							<div class="absolute top-11 right-0 z-10 min-w-36 rounded-md border border-line bg-cream p-1 shadow-cover-hover">
								<button class="w-full cursor-pointer rounded-sm border-0 bg-transparent px-3 py-2 text-left font-[inherit] text-sm font-bold text-ink hover:bg-sage focus-visible:bg-sage focus-visible:outline-3 focus-visible:outline-clay" type="button" onclick={(event) => {
									event.currentTarget.closest('details')?.removeAttribute('open');
									openIdentification(book);
								}}>Identify book</button>
							</div>
						</details>
					</article>
				{/each}
			</section>
		{/if}
	{/if}
</main>

{#if editingHighlight}
	<dialog open aria-labelledby="edit-highlight-title" class="m-auto w-[min(42rem,calc(100%-2rem))] rounded-lg border border-line bg-cream p-0 text-ink shadow-cover-hover backdrop:bg-ink/60">
		<form class="p-[clamp(1.25rem,4vw,2rem)]" onsubmit={(event) => { event.preventDefault(); saveHighlight(); }}>
			<div class="mb-5 flex items-start justify-between gap-4">
				<h2 class="font-serif text-3xl leading-tight" id="edit-highlight-title">Edit highlight</h2>
				<button class="grid h-10 w-10 shrink-0 cursor-pointer place-items-center rounded-full border border-line bg-transparent text-xl hover:bg-sage focus-visible:outline-3 focus-visible:outline-offset-2 focus-visible:outline-clay" type="button" aria-label="Close edit highlight" onclick={() => (editingHighlight = null)}>×</button>
			</div>
			<label class="mb-2 block font-bold" for="highlight-content">Highlight text</label>
			<textarea class="min-h-44 w-full rounded-md border border-[#aeb9a6] bg-white px-3 py-2.5 font-[inherit] leading-relaxed focus-visible:outline-3 focus-visible:outline-offset-2 focus-visible:outline-clay" id="highlight-content" bind:value={editedContent} required></textarea>
			{#if editError}<p class="mt-3 text-danger" role="alert">{editError}</p>{/if}
			<div class="mt-5 flex justify-end gap-3">
				<button class="cursor-pointer rounded-md border border-[#aeb9a6] bg-transparent px-4 py-2.5 font-[inherit] font-bold text-forest hover:bg-sage focus-visible:outline-3 focus-visible:outline-offset-2 focus-visible:outline-clay" type="button" disabled={savingHighlight} onclick={() => (editingHighlight = null)}>Cancel</button>
				<button class="cursor-pointer rounded-md border border-forest bg-forest px-4 py-2.5 font-[inherit] font-bold text-cream hover:bg-forest-dark focus-visible:outline-3 focus-visible:outline-offset-2 focus-visible:outline-clay disabled:cursor-wait disabled:opacity-60" type="submit" disabled={savingHighlight}>Save</button>
			</div>
		</form>
	</dialog>
{/if}

<dialog bind:this={identificationDialog} aria-labelledby="identification-title" onclose={() => {
	searchRequest += 1;
	searching = false;
	identifyingBook = null;
}} class="m-auto max-h-[min(48rem,calc(100vh-2rem))] w-[min(42rem,calc(100%-2rem))] overflow-y-auto rounded-lg border border-line bg-cream p-0 text-ink shadow-cover-hover backdrop:bg-ink/60">
	<div class="p-[clamp(1.25rem,4vw,2rem)]">
		<div class="mb-5 flex items-start justify-between gap-4">
			<div>
				<p class="mb-1 text-xs font-extrabold tracking-[0.14em] text-clay uppercase">Open Library</p>
				<h2 class="font-serif text-3xl leading-tight" id="identification-title">Identify book</h2>
			</div>
			<button class="grid h-10 w-10 shrink-0 cursor-pointer place-items-center rounded-full border border-line bg-transparent text-xl hover:bg-sage focus-visible:outline-3 focus-visible:outline-offset-2 focus-visible:outline-clay" type="button" aria-label="Close identification dialog" onclick={() => identificationDialog.close()}>×</button>
		</div>

		<form class="flex gap-2 max-sm:flex-col" aria-labelledby="identification-title" onsubmit={(event) => {
			event.preventDefault();
			searchBooks();
		}}>
			<label class="sr-only" for="book-search">Book title</label>
			<input bind:this={searchInput} class="min-w-0 flex-1 rounded-md border border-[#aeb9a6] bg-white px-3 py-2.5 font-[inherit] focus-visible:outline-3 focus-visible:outline-offset-2 focus-visible:outline-clay" id="book-search" name="q" bind:value={searchQuery} required />
			<button class="cursor-pointer rounded-md border border-forest bg-forest px-4 py-2.5 font-[inherit] font-bold text-cream hover:bg-forest-dark focus-visible:outline-3 focus-visible:outline-offset-2 focus-visible:outline-clay disabled:cursor-wait disabled:opacity-60" type="submit" disabled={searching || saving}>Search</button>
		</form>

		<div class="mt-6" aria-live="polite" aria-busy={searching}>
			{#if searching}
				<p class="py-10 text-center text-muted" role="status">Searching Open Library…</p>
			{:else if identificationError}
				<p class="rounded-md bg-[#f9e8e2] p-3 text-danger" role="alert">{identificationError}</p>
			{:else if searchResults.length}
				<ul class="grid gap-3">
					{#each searchResults as candidate}
						<li class="flex gap-4 rounded-md border border-line p-3 max-sm:gap-3">
							{#if candidate.cover_id}
								<img class="h-28 w-[4.7rem] shrink-0 rounded-sm object-cover" src={`https://covers.openlibrary.org/b/id/${candidate.cover_id}-M.jpg`} alt="" width="75" height="112" loading="lazy" />
							{:else}
								<div class="grid h-28 w-[4.7rem] shrink-0 place-items-center rounded-sm bg-forest font-serif text-2xl text-cream" aria-hidden="true">{candidate.title.slice(0, 1)}</div>
							{/if}
							<div class="min-w-0 flex-1">
								<h3 class="font-serif text-lg leading-tight break-words">{candidate.title}</h3>
								{#if candidate.authors.length}<p class="mt-1 text-sm text-muted">{candidate.authors.join(', ')}</p>{/if}
								<p class="mt-2 text-xs text-muted-light">
									{candidate.first_publish_year ? `First published ${candidate.first_publish_year}` : 'Publication year unknown'}
									{#if candidate.edition_count !== null} · {candidate.edition_count} edition{candidate.edition_count === 1 ? '' : 's'}{/if}
								</p>
								<button class="mt-3 cursor-pointer rounded-full border border-forest bg-transparent px-3 py-1.5 font-[inherit] text-sm font-bold text-forest hover:bg-sage focus-visible:outline-3 focus-visible:outline-offset-2 focus-visible:outline-clay disabled:cursor-wait disabled:opacity-60" type="button" disabled={saving} onclick={() => identifyBook(candidate)}>Identify</button>
							</div>
						</li>
					{/each}
				</ul>
			{:else}
				<p class="py-10 text-center text-muted">No matching books found.</p>
			{/if}
		</div>
	</div>
</dialog>
