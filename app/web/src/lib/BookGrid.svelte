<script lang="ts">
	import type { BookSummary } from '$lib/types';

	let {
		books,
		compact = false,
		onidentify
	}: { books: BookSummary[]; compact?: boolean; onidentify: (book: BookSummary) => void } = $props();
</script>

<section class={`${compact ? 'mt-4' : 'mt-8'} grid grid-cols-[repeat(auto-fill,minmax(min(100%,10rem),1fr))] gap-x-[clamp(0.9rem,2.5vw,1.5rem)] gap-y-[clamp(1.25rem,3vw,2rem)]`} aria-label="Books with highlights">
	{#each books as book (book.id)}
		<article class="relative min-w-0">
			<a class="group block no-underline w-full min-w-0 border border-transparent bg-transparent p-0 text-left font-[inherit] text-inherit focus-visible:outline-3 focus-visible:outline-offset-4 focus-visible:outline-clay" href={`/?book=${book.id}`}>
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
					<div class="mb-3.5 grid aspect-2/3 w-full place-items-center rounded-sm bg-forest font-serif text-[clamp(2.5rem,6vw,4rem)] text-on-forest shadow-cover transition duration-150 group-hover:-translate-y-1 group-hover:shadow-cover-hover group-focus-visible:-translate-y-1 group-focus-visible:shadow-cover-hover motion-reduce:transition-none" aria-hidden="true">{book.title.slice(0, 1)}</div>
				{/if}
				<span class="line-clamp-2 min-w-0 font-bold leading-[1.3] break-words">{book.title}</span>
				{#if book.authors.length}<span class="mt-1 line-clamp-1 min-w-0 text-sm leading-[1.3] break-words text-muted">{book.authors.join(', ')}</span>{/if}
				<span class="mt-2.5 inline-block rounded-full bg-chip px-2 py-1 text-xs leading-tight font-bold text-chip-text">{book.highlight_count} highlight{book.highlight_count === 1 ? '' : 's'}</span>
			</a>
			<details class="absolute top-2 right-2">
				<summary class="grid h-10 w-10 cursor-pointer list-none place-items-center rounded-full border border-line bg-cream/95 text-xl leading-none font-bold shadow-sm hover:bg-sage focus-visible:outline-3 focus-visible:outline-offset-2 focus-visible:outline-clay [&::-webkit-details-marker]:hidden" aria-label={`Actions for ${book.title}`}>•••</summary>
				<div class="absolute top-11 right-0 z-10 min-w-36 rounded-md border border-line bg-cream p-1 shadow-cover-hover">
					<button class="w-full cursor-pointer rounded-sm border-0 bg-transparent px-3 py-2 text-left font-[inherit] text-sm font-bold text-ink hover:bg-sage focus-visible:bg-sage focus-visible:outline-3 focus-visible:outline-clay" type="button" onclick={(event) => {
						event.currentTarget.closest('details')?.removeAttribute('open');
						onidentify(book);
					}}>Identify book</button>
				</div>
			</details>
		</article>
	{/each}
</section>
