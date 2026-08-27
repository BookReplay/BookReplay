<script lang="ts">
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
			message = 'Choose a non-empty .txt file.';
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
				.catch(() => ({ message: 'Clippings import failed.' }))) as ImportResponse;

			success = response.ok;
			message = response.ok
				? `Imported ${result.inserted ?? 0} of ${result.parsed ?? 0} clippings (${result.duplicates ?? 0} duplicates).`
				: (result.message ?? 'Clippings import failed.');
		} catch {
			success = false;
			message = 'Could not reach the clippings API.';
		} finally {
			uploading = false;
		}
	}
</script>

<svelte:head>
	<title>Rekindle</title>
	<meta name="description" content="Import Kindle clippings" />
</svelte:head>

<main>
	<form onsubmit={upload}>
		<h1>Import Kindle clippings</h1>
		<p>Choose your <code>My Clippings.txt</code> file.</p>

		<label>
			<span>Clippings file</span>
			<input name="clippings" type="file" accept=".txt,text/plain" required />
		</label>

		<button type="submit" disabled={uploading}>{uploading ? 'Uploading…' : 'Upload'}</button>

		{#if message}
			<p class:success role="status">{message}</p>
		{/if}
	</form>
</main>

<style>
	:global(*) {
		box-sizing: border-box;
	}

	:global(body) {
		margin: 0;
		background: #f5f1e8;
		color: #29261f;
		font-family: system-ui, sans-serif;
	}

	main {
		display: grid;
		min-height: 100vh;
		place-items: center;
		padding: 1.5rem;
	}

	form {
		width: min(100%, 32rem);
		padding: 2rem;
		border: 1px solid #d9d0bf;
		border-radius: 0.75rem;
		background: #fffdf8;
		box-shadow: 0 1rem 3rem rgb(80 65 40 / 10%);
	}

	h1 {
		margin: 0 0 0.5rem;
		font-family: Georgia, serif;
		font-size: clamp(1.75rem, 5vw, 2.25rem);
	}

	p {
		margin: 0 0 1.5rem;
		color: #686052;
	}

	label,
	span {
		display: block;
	}

	span {
		margin-bottom: 0.5rem;
		font-weight: 650;
	}

	input {
		width: 100%;
		padding: 0.75rem;
		border: 1px dashed #9e8e75;
		border-radius: 0.5rem;
		background: #faf7ef;
	}

	button {
		width: 100%;
		margin-top: 1rem;
		padding: 0.8rem 1rem;
		border: 0;
		border-radius: 0.5rem;
		background: #4c5f46;
		color: white;
		font: inherit;
		font-weight: 700;
		cursor: pointer;
	}

	button:hover {
		background: #3d4d38;
	}

	button:disabled {
		cursor: wait;
		opacity: 0.7;
	}

	[role='status'] {
		margin: 1rem 0 0;
		color: #a33b2f;
	}

	[role='status'].success {
		color: #386633;
	}
</style>
