<script lang="ts">
	import EditHighlightDialog from '$lib/EditHighlightDialog.svelte';
	import type { BookHighlight, BookSummary } from '$lib/types';

	let { book }: { book: BookSummary } = $props();

	let editDialog: EditHighlightDialog;
	let highlights = $state<BookHighlight[]>([]);
	let loadingHighlights = $state(true);
	let highlightsError = $state('');
	let highlightsRequest = 0;

	$effect(() => {
		void loadHighlights(book.id);
	});

	async function loadHighlights(bookId: number) {
		// Only the latest request may update the view when books are opened in quick succession.
		const request = ++highlightsRequest;
		highlights = [];
		highlightsError = '';
		loadingHighlights = true;

		try {
			const response = await fetch(`/api/books/${bookId}/clippings`);
			if (!response.ok) throw new Error();

			const loaded = (await response.json()) as BookHighlight[];
			if (request === highlightsRequest) highlights = loaded;
		} catch {
			if (request === highlightsRequest) highlightsError = 'Could not load this book’s highlights. Try again.';
		} finally {
			if (request === highlightsRequest) loadingHighlights = false;
		}
	}
</script>

<a class="mt-12 mb-6 inline-block no-underline font-bold text-link hover:text-clay focus-visible:outline-3 focus-visible:outline-offset-4 focus-visible:outline-clay" href="/?view=books">← All books</a>
<header class="flex items-end gap-[clamp(1.25rem,4vw,2.5rem)] border-b border-line pb-10 max-sm:items-start">
	{#if book.cover_url}
		<img
			class="h-60 w-40 shrink-0 rounded-sm object-cover shadow-cover max-sm:h-36 max-sm:w-24"
			src={book.cover_url}
			alt={`Cover of ${book.title}`}
			width="160"
			height="240"
		/>
	{:else}
		<div class="grid h-60 w-40 shrink-0 place-items-center rounded-sm bg-forest font-serif text-[clamp(2.5rem,6vw,4rem)] text-on-forest shadow-cover max-sm:h-36 max-sm:w-24" aria-hidden="true">{book.title.slice(0, 1)}</div>
	{/if}
	<div class="min-w-0">
		<p class="mb-2.5 text-xs font-extrabold tracking-[0.14em] text-clay uppercase">Selected book</p>
		<h1 class="mb-3 max-w-[38rem] font-serif text-[clamp(2.25rem,6vw,4.5rem)] leading-[0.98] tracking-[-0.04em] text-balance break-words">{book.title}</h1>
		{#if book.authors.length}
			<p class="mb-0 text-[1.05rem] text-muted">{book.authors.join(', ')}</p>
		{/if}
		<span class="mt-2.5 inline-block rounded-full bg-chip px-2 py-1 text-xs leading-tight font-bold text-chip-text">{book.highlight_count} highlight{book.highlight_count === 1 ? '' : 's'}</span>
	</div>
</header>

{#if loadingHighlights}
	<p class="mt-[30vh] text-center text-muted" role="status" aria-live="polite">Loading highlights…</p>
{:else if highlightsError}
	<p class="mt-[30vh] text-center text-danger" role="alert" aria-live="polite">{highlightsError}</p>
{:else}
	<section class="mx-auto mt-10 grid w-full max-w-3xl gap-4" aria-label={`Highlights from ${book.title}`}>
		{#each highlights as highlight (highlight.id)}
			<article class="rounded-lg border border-line bg-cream p-[clamp(1.25rem,4vw,2rem)] shadow-[0_0.4rem_1.3rem_rgb(80_65_40/6%)]">
				<div class="mb-5 flex items-start justify-between gap-4">
					<blockquote class="m-0 font-serif text-[clamp(1.05rem,2vw,1.2rem)] leading-[1.7] break-words">{highlight.content}</blockquote>
					<button class="shrink-0 cursor-pointer rounded-md border border-input-border bg-transparent px-3 py-1.5 font-[inherit] text-sm font-bold text-link hover:border-forest hover:bg-sage focus-visible:outline-3 focus-visible:outline-offset-2 focus-visible:outline-clay" type="button" onclick={() => editDialog.open(highlight)}>Edit</button>
				</div>
				<footer class="text-xs text-muted-light break-words">{highlight.metadata}</footer>
			</article>
		{/each}
	</section>
{/if}

<EditHighlightDialog
	bind:this={editDialog}
	onsaved={(id, content) =>
		(highlights = highlights.map((highlight) => (highlight.id === id ? { ...highlight, content } : highlight)))}
/>
