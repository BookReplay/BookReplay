<script lang="ts">
	import { onDestroy } from 'svelte';
	import LogoutButton from '$lib/LogoutButton.svelte';

	// Matches the server's import limit (MAX_IMPORT_BYTES).
	const MAX_FILE_BYTES = 16 * 1024 * 1024;
	const TOO_LARGE = 'That file is larger than 16 MB, the most BookReplay accepts in one import.';

	type ImportResponse = {
		error?: string;
		parsed?: number;
		inserted?: number;
		duplicates?: number;
		preview?: string;
	};

	let message = $state('');
	let success = $state(false);
	let uploading = $state(false);
	let importedCount = $state(0);
	let skippedCount = $state(0);
	let preview = $state('');
	let showingPreview = $state(false);
	let previewTimer: ReturnType<typeof setTimeout> | undefined;

	onDestroy(() => clearTimeout(previewTimer));

	async function upload(event: SubmitEvent) {
		event.preventDefault();

		const form = event.currentTarget as HTMLFormElement;
		const input = form.elements.namedItem('clippings');
		const file = input instanceof HTMLInputElement ? input.files?.[0] : undefined;

		if (!file || file.size === 0 || !file.name.toLowerCase().endsWith('.txt')) {
			success = false;
			message = 'Choose a non-empty .txt file, then try again.';
			return;
		}
		if (file.size > MAX_FILE_BYTES) {
			success = false;
			message = TOO_LARGE;
			return;
		}

		uploading = true;
		message = '';
		showingPreview = false;
		clearTimeout(previewTimer);

		try {
			const response = await fetch('/api/clippings/import', {
				method: 'POST',
				headers: { 'content-type': 'text/plain; charset=utf-8' },
				body: await file.text()
			});
			const result = (await response.json().catch(() => ({}))) as ImportResponse;

			success = response.ok;
			if (response.ok) {
				importedCount = result.inserted ?? 0;
				skippedCount = result.duplicates ?? 0;
				preview = result.preview ?? 'Your ideas are arriving.';
				showingPreview = true;
				previewTimer = setTimeout(() => (showingPreview = false), 900);
			} else {
				message =
					response.status === 413
						? TOO_LARGE
						: 'Clippings import failed. Check the file and try again.';
			}
		} catch {
			success = false;
			message = 'Could not reach BookReplay. Check your connection and try again.';
		} finally {
			uploading = false;
		}
	}
</script>

<svelte:head>
	<title>Import Clippings · BookReplay</title>
	<meta name="description" content="Import a Kindle clippings file into BookReplay" />
</svelte:head>

<a class="absolute top-3 left-3 z-10 -translate-y-[200%] rounded-md bg-ink px-3 py-2 text-cream focus-visible:translate-y-0" href="#main-content">Skip to content</a>

<main id="main-content" class="mx-auto min-h-screen w-[calc(100%-2rem)] max-w-6xl pt-5 pb-20 max-sm:w-[calc(100%-1.5rem)]">
	<nav class="flex items-center justify-between gap-4" aria-label="Primary navigation">
		<a class="font-serif text-[1.4rem] font-bold text-ink no-underline focus-visible:outline-3 focus-visible:outline-offset-3 focus-visible:outline-clay" href="/">BookReplay</a>
		<div class="flex items-center gap-4">
			<a class="text-sm font-bold text-forest focus-visible:outline-3 focus-visible:outline-offset-3 focus-visible:outline-clay" href="/">← Your bookshelf</a>
			<LogoutButton />
		</div>
	</nav>

	<section class="mx-auto mt-[clamp(4rem,12vw,9rem)] w-full max-w-[38rem] rounded-xl border border-line bg-cream p-[clamp(1.5rem,6vw,3.5rem)] shadow-[0_1.25rem_3rem_rgb(80_65_40/10%)]" aria-live="polite">
		{#if showingPreview}
			<div class="import-preview text-center">
				<p class="mb-5 text-xs font-extrabold tracking-[0.14em] text-clay uppercase">A highlight, returning</p>
				<blockquote class="m-0 font-serif text-[clamp(1.45rem,4vw,2.25rem)] leading-relaxed text-balance">“{preview}”</blockquote>
			</div>
		{:else if success}
			<div class="text-center">
				<p class="mb-4 text-3xl text-clay" aria-hidden="true">✦</p>
				<h1 class="mb-3 font-serif text-[clamp(2.25rem,8vw,4.5rem)] leading-[0.98] tracking-[-0.04em] text-balance">Your highlights are imported</h1>
				<p class="mb-8 text-[1.05rem] leading-relaxed text-muted">{importedCount} highlight{importedCount === 1 ? '' : 's'} imported{skippedCount ? `; ${skippedCount} already in your library` : ''}. Ready to revisit them?</p>
				<a class="inline-block rounded-full border border-forest bg-forest px-6 py-3.5 font-bold text-cream no-underline shadow-cover hover:bg-forest-dark focus-visible:outline-3 focus-visible:outline-offset-4 focus-visible:outline-clay" href="/review/">Start revision <span aria-hidden="true">→</span></a>
			</div>
		{:else}
			<p class="mb-2.5 text-xs font-extrabold tracking-[0.14em] text-clay uppercase">Add to your library</p>
			<h1 id="import-title" class="mb-4 font-serif text-[clamp(2.25rem,8vw,4.5rem)] leading-[0.98] tracking-[-0.04em] text-balance">Bring your highlights home</h1>
			<p class="m-0 text-[1.05rem] leading-relaxed text-muted">Choose the <code class="text-[0.9em]">My Clippings.txt</code> file from your Kindle. Your existing highlights will stay safe; duplicates are skipped.</p>

			<form class="mt-10" onsubmit={upload} aria-describedby="file-help">
			<label class="mb-2 block font-bold" for="clippings">Clippings file</label>
			<p id="file-help" class="mb-3 text-sm text-muted-light">A plain-text <code class="text-[0.9em]">.txt</code> export from your Kindle.</p>
			<input class="w-full rounded-md border border-dashed border-[#9e8e75] bg-[#faf7ef] p-3 font-[inherit] focus-visible:outline-3 focus-visible:outline-offset-3 focus-visible:outline-clay" id="clippings" name="clippings" type="file" accept=".txt,text/plain" autocomplete="off" required />
			<button class="mt-4 w-full cursor-pointer rounded-md border border-forest bg-forest px-4 py-3.5 font-[inherit] font-bold text-cream hover:bg-forest-dark focus-visible:outline-3 focus-visible:outline-offset-3 focus-visible:outline-clay disabled:cursor-wait disabled:opacity-70" type="submit" disabled={uploading}>{uploading ? 'Importing…' : 'Import clippings'}</button>

			{#if message}
				<p class="mt-4 leading-relaxed text-danger" role="alert">{message}</p>
			{/if}
			</form>
		{/if}
	</section>
</main>
