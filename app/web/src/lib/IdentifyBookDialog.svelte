<script lang="ts">
	import { tick } from 'svelte';
	import type { BookCandidate, BookSummary } from '$lib/types';

	const DISABLED = 'Metadata lookup is turned off on this BookReplay instance.';

	let { onidentified }: { onidentified: (book: BookSummary) => void } = $props();

	let identificationDialog: HTMLDialogElement;
	let searchInput: HTMLInputElement;
	let identifyingBook = $state<BookSummary | null>(null);
	let searchQuery = $state('');
	let searchResults = $state<BookCandidate[]>([]);
	let searching = $state(false);
	let saving = $state(false);
	let identificationError = $state('');
	let searchRequest = 0;

	export async function open(book: BookSummary) {
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
			if (response.status === 403) throw new Error(DISABLED);
			if (!response.ok) throw new Error();
			const found = (await response.json()) as BookCandidate[];
			if (request === searchRequest) searchResults = found;
		} catch (error) {
			if (request === searchRequest)
				identificationError =
					error instanceof Error && error.message === DISABLED ? DISABLED : 'Could not search books. Try again.';
		} finally {
			if (request === searchRequest) searching = false;
		}
	}

	async function identifyBook(candidate: BookCandidate) {
		if (!identifyingBook) return;

		saving = true;
		identificationError = '';
		try {
			const response = await fetch(`/api/books/${identifyingBook.id}/identification`, {
				method: 'PUT',
				headers: { 'content-type': 'application/json' },
				body: JSON.stringify(candidate)
			});
			if (!response.ok) throw new Error();

			onidentified((await response.json()) as BookSummary);
			identificationDialog.close();
		} catch {
			identificationError = 'Could not identify this book. Try again.';
		} finally {
			saving = false;
		}
	}
</script>

<dialog bind:this={identificationDialog} aria-labelledby="identification-title" onclose={() => {
	searchRequest += 1;
	searching = false;
	identifyingBook = null;
}} class="m-auto max-h-[min(48rem,calc(100vh-2rem))] w-[min(42rem,calc(100%-2rem))] overflow-y-auto rounded-lg border border-line bg-cream p-0 text-ink shadow-cover-hover backdrop:bg-ink/60">
	<div class="p-[clamp(1.25rem,4vw,2rem)]">
		<div class="mb-5 flex items-start justify-between gap-4">
			<div>
				<p class="mb-1 text-xs font-extrabold tracking-[0.14em] text-clay uppercase">Book metadata</p>
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
				<p class="py-10 text-center text-muted" role="status">Searching books…</p>
			{:else if identificationError}
				<p class="rounded-md bg-[#f9e8e2] p-3 text-danger" role="alert">{identificationError}</p>
			{:else if searchResults.length}
				<ul class="grid gap-3">
					{#each searchResults as candidate (candidate.provider + candidate.provider_id)}
						<li class="flex gap-4 rounded-md border border-line p-3 max-sm:gap-3">
							{#if candidate.cover_url}
								<img class="h-28 w-[4.7rem] shrink-0 rounded-sm object-cover" src={candidate.cover_url} alt="" width="75" height="112" loading="lazy" />
							{:else}
								<div class="grid h-28 w-[4.7rem] shrink-0 place-items-center rounded-sm bg-forest font-serif text-2xl text-cream" aria-hidden="true">{candidate.title.slice(0, 1)}</div>
							{/if}
							<div class="min-w-0 flex-1">
								<p class="mb-1 text-xs text-muted">{candidate.provider === 'open_library' ? 'Open Library' : 'Google Books'}</p>
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
