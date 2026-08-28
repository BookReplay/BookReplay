<script lang="ts">
	import LogoutButton from '$lib/LogoutButton.svelte';

	type ImportResponse = {
		message?: string;
		parsed?: number;
		inserted?: number;
		duplicates?: number;
	};

	let message = $state('');
	let success = $state(false);
	let uploading = $state(false);

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

		uploading = true;
		message = '';

		try {
			const response = await fetch('/api/clippings/import', {
				method: 'POST',
				headers: { 'content-type': 'text/plain; charset=utf-8' },
				body: await file.text()
			});
			const result = (await response
				.json()
				.catch(() => ({ message: 'Clippings import failed. Check the file and try again.' }))) as ImportResponse;

			success = response.ok;
			message = response.ok
				? `Imported ${result.inserted ?? 0} of ${result.parsed ?? 0} clippings (${result.duplicates ?? 0} duplicates).`
				: (result.message ?? 'Clippings import failed. Check the file and try again.');
		} catch {
			success = false;
			message = 'Could not reach the clippings API. Check your connection and try again.';
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

	<section class="mx-auto mt-[clamp(4rem,12vw,9rem)] w-full max-w-[38rem] rounded-xl border border-line bg-cream p-[clamp(1.5rem,6vw,3.5rem)] shadow-[0_1.25rem_3rem_rgb(80_65_40/10%)]" aria-labelledby="import-title">
		<p class="mb-2.5 text-xs font-extrabold tracking-[0.14em] text-clay uppercase">Add to your library</p>
		<h1 id="import-title" class="mb-4 font-serif text-[clamp(2.25rem,8vw,4.5rem)] leading-[0.98] tracking-[-0.04em] text-balance">Bring your highlights home</h1>
		<p class="m-0 text-[1.05rem] leading-relaxed text-muted">Choose the <code class="text-[0.9em]">My Clippings.txt</code> file from your Kindle. Your existing highlights will stay safe; duplicates are skipped.</p>

		<form class="mt-10" onsubmit={upload} aria-describedby="file-help">
			<label class="mb-2 block font-bold" for="clippings">Clippings file</label>
			<p id="file-help" class="mb-3 text-sm text-muted-light">A plain-text <code class="text-[0.9em]">.txt</code> export from your Kindle.</p>
			<input class="w-full rounded-md border border-dashed border-[#9e8e75] bg-[#faf7ef] p-3 font-[inherit] focus-visible:outline-3 focus-visible:outline-offset-3 focus-visible:outline-clay" id="clippings" name="clippings" type="file" accept=".txt,text/plain" autocomplete="off" required />
			<button class="mt-4 w-full cursor-pointer rounded-md border border-forest bg-forest px-4 py-3.5 font-[inherit] font-bold text-cream hover:bg-forest-dark focus-visible:outline-3 focus-visible:outline-offset-3 focus-visible:outline-clay disabled:cursor-wait disabled:opacity-70" type="submit" disabled={uploading}>{uploading ? 'Importing…' : 'Import clippings'}</button>

			{#if message}
				<p class={`mt-4 leading-relaxed ${success ? 'text-success' : 'text-danger'}`} role={success ? 'status' : 'alert'} aria-live="polite">{message}</p>
			{/if}
		</form>
	</section>
</main>
