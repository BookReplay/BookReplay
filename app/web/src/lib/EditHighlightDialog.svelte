<script lang="ts">
	import type { BookHighlight } from '$lib/types';

	let { onsaved }: { onsaved: (id: number, content: string) => void } = $props();

	let editDialog: HTMLDialogElement;
	let editInput: HTMLTextAreaElement;
	let editingHighlight = $state<BookHighlight | null>(null);
	let editedContent = $state('');
	let savingHighlight = $state(false);
	let editError = $state('');

	export function open(highlight: BookHighlight) {
		editingHighlight = highlight;
		editedContent = highlight.content;
		editError = '';
		editDialog.showModal();
		editInput.focus();
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
			onsaved(editingHighlight.id, content);
			editDialog.close();
		} catch {
			editError = 'Could not save this highlight. Try again.';
		} finally {
			savingHighlight = false;
		}
	}
</script>

<dialog bind:this={editDialog} aria-labelledby="edit-highlight-title" onclose={() => (editingHighlight = null)} class="m-auto w-[min(42rem,calc(100%-2rem))] rounded-lg border border-line bg-cream p-0 text-ink shadow-cover-hover backdrop:bg-ink/60">
	<form class="p-[clamp(1.25rem,4vw,2rem)]" onsubmit={(event) => { event.preventDefault(); saveHighlight(); }}>
		<div class="mb-5 flex items-start justify-between gap-4">
			<h2 class="font-serif text-3xl leading-tight" id="edit-highlight-title">Edit highlight</h2>
			<button class="grid h-10 w-10 shrink-0 cursor-pointer place-items-center rounded-full border border-line bg-transparent text-xl hover:bg-sage focus-visible:outline-3 focus-visible:outline-offset-2 focus-visible:outline-clay" type="button" aria-label="Close edit highlight" onclick={() => editDialog.close()}>×</button>
		</div>
		<label class="mb-2 block font-bold" for="highlight-content">Highlight text</label>
		<textarea class="min-h-44 w-full rounded-md border border-[#aeb9a6] bg-white px-3 py-2.5 font-[inherit] leading-relaxed focus-visible:outline-3 focus-visible:outline-offset-2 focus-visible:outline-clay" id="highlight-content" bind:this={editInput} bind:value={editedContent} required></textarea>
		{#if editError}<p class="mt-3 text-danger" role="alert">{editError}</p>{/if}
		<div class="mt-5 flex justify-end gap-3">
			<button class="cursor-pointer rounded-md border border-[#aeb9a6] bg-transparent px-4 py-2.5 font-[inherit] font-bold text-forest hover:bg-sage focus-visible:outline-3 focus-visible:outline-offset-2 focus-visible:outline-clay" type="button" disabled={savingHighlight} onclick={() => editDialog.close()}>Cancel</button>
			<button class="cursor-pointer rounded-md border border-forest bg-forest px-4 py-2.5 font-[inherit] font-bold text-cream hover:bg-forest-dark focus-visible:outline-3 focus-visible:outline-offset-2 focus-visible:outline-clay disabled:cursor-wait disabled:opacity-60" type="submit" disabled={savingHighlight}>Save</button>
		</div>
	</form>
</dialog>
