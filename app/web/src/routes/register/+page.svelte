<script lang="ts">
	let message = $state('');
	let submitting = $state(false);

	async function register(event: SubmitEvent) {
		event.preventDefault();
		const form = event.currentTarget as HTMLFormElement;
		const data = new FormData(form);

		submitting = true;
		message = '';
		try {
			const response = await fetch('/api/auth/register', {
				method: 'POST',
				headers: { 'content-type': 'application/json' },
				body: JSON.stringify({
					setup_secret: data.get('setup_secret'),
					email: data.get('email'),
					name: data.get('name'),
					password: data.get('password')
				})
			});
			const result = (await response.json().catch(() => ({}))) as { error?: string };
			if (!response.ok) {
				message = response.status === 429 ? 'Too many attempts. Wait one minute and try again.' : result.error ?? 'Registration failed. Try again.';
				return;
			}

			window.location.assign(response.headers.get('location') ?? '/login/');
		} catch {
			message = 'Could not reach BookReplay. Check your connection and try again.';
		} finally {
			submitting = false;
		}
	}
</script>

<svelte:head>
	<title>Create owner · BookReplay</title>
	<meta name="description" content="Create the owner account for BookReplay" />
</svelte:head>

<a class="absolute top-3 left-3 z-10 -translate-y-[200%] rounded-md bg-ink px-3 py-2 text-cream focus-visible:translate-y-0" href="#main-content">Skip to content</a>

<main id="main-content" class="mx-auto grid min-h-screen w-[calc(100%-2rem)] max-w-6xl place-items-center py-10 max-sm:w-[calc(100%-1.5rem)]">
	<section class="w-full max-w-[34rem] rounded-xl border border-line bg-cream p-[clamp(1.5rem,6vw,3.5rem)] shadow-[0_1.25rem_3rem_rgb(80_65_40/10%)]" aria-labelledby="register-title">
		<p class="mb-2.5 text-xs font-extrabold tracking-[0.14em] text-clay uppercase">Welcome to BookReplay</p>
		<h1 id="register-title" class="mb-4 font-serif text-[clamp(2.25rem,8vw,4.5rem)] leading-[0.98] tracking-[-0.04em] text-balance">Create your owner account</h1>
		<p class="m-0 leading-relaxed text-muted">This first account will be the only owner of this BookReplay library.</p>

		<form class="mt-9 grid gap-5" onsubmit={register}>
			<div>
				<label class="mb-2 block font-bold" for="setup-secret">Setup secret</label>
				<p id="setup-help" class="mb-2 text-sm text-muted-light">Enter the secret configured by the server operator.</p>
				<input class="w-full rounded-md border border-line bg-input px-3.5 py-3" id="setup-secret" name="setup_secret" type="password" autocomplete="off" maxlength="128" aria-describedby="setup-help" required />
			</div>
			<div>
				<label class="mb-2 block font-bold" for="name">Name</label>
				<input class="w-full rounded-md border border-line bg-input px-3.5 py-3 font-[inherit] focus-visible:outline-3 focus-visible:outline-offset-2 focus-visible:outline-clay" id="name" name="name" type="text" autocomplete="name" maxlength="100" required />
			</div>
			<div>
				<label class="mb-2 block font-bold" for="email">Email</label>
				<input class="w-full rounded-md border border-line bg-input px-3.5 py-3 font-[inherit] focus-visible:outline-3 focus-visible:outline-offset-2 focus-visible:outline-clay" id="email" name="email" type="email" autocomplete="email" maxlength="254" required />
			</div>
			<div>
				<label class="mb-2 block font-bold" for="password">Password</label>
				<p id="password-help" class="mb-2 text-sm text-muted-light">Use 12–128 characters. Accented letters and emoji count as more than one.</p>
				<input class="w-full rounded-md border border-line bg-input px-3.5 py-3 font-[inherit] focus-visible:outline-3 focus-visible:outline-offset-2 focus-visible:outline-clay" id="password" name="password" type="password" autocomplete="new-password" minlength="12" maxlength="128" aria-describedby="password-help" required />
			</div>

			<button class="w-full cursor-pointer rounded-md border border-forest bg-forest px-4 py-3.5 font-[inherit] font-bold text-on-forest hover:bg-forest-dark focus-visible:outline-3 focus-visible:outline-offset-3 focus-visible:outline-clay disabled:cursor-wait disabled:opacity-70" type="submit" disabled={submitting}>{submitting ? 'Creating account…' : 'Create owner account'}</button>
			{#if message}
				<p class="m-0 leading-relaxed text-danger" role="alert" aria-live="polite">{message}</p>
			{/if}
		</form>
	</section>
</main>
