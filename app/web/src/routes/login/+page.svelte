<script lang="ts">
	let message = $state('');
	let submitting = $state(false);

	async function login(event: SubmitEvent) {
		event.preventDefault();
		const form = event.currentTarget as HTMLFormElement;
		const data = new FormData(form);

		submitting = true;
		message = '';
		try {
			const response = await fetch('/api/auth/login', {
				method: 'POST',
				headers: { 'content-type': 'application/json' },
				body: JSON.stringify({
					email: data.get('email'),
					password: data.get('password')
				})
			});
			if (!response.ok) {
				const result = (await response.json().catch(() => ({}))) as { error?: string };
				message = result.error ?? 'Sign in failed. Try again.';
				return;
			}

			window.location.assign('/');
		} catch {
			message = 'Could not reach Rekindle. Check your connection and try again.';
		} finally {
			submitting = false;
		}
	}
</script>

<svelte:head>
	<title>Sign in · Rekindle</title>
	<meta name="description" content="Sign in to your Rekindle library" />
</svelte:head>

<a class="absolute top-3 left-3 z-10 -translate-y-[200%] rounded-md bg-ink px-3 py-2 text-cream focus-visible:translate-y-0" href="#main-content">Skip to content</a>

<main id="main-content" class="mx-auto grid min-h-screen w-[calc(100%-2rem)] max-w-6xl place-items-center py-10 max-sm:w-[calc(100%-1.5rem)]">
	<section class="w-full max-w-[34rem] rounded-xl border border-line bg-cream p-[clamp(1.5rem,6vw,3.5rem)] shadow-[0_1.25rem_3rem_rgb(80_65_40/10%)]" aria-labelledby="login-title">
		<p class="mb-2.5 text-xs font-extrabold tracking-[0.14em] text-clay uppercase">Your library awaits</p>
		<h1 id="login-title" class="mb-4 font-serif text-[clamp(2.25rem,8vw,4.5rem)] leading-[0.98] tracking-[-0.04em] text-balance">Sign in to Rekindle</h1>
		<p class="m-0 leading-relaxed text-muted">Return to your books and highlights.</p>

		<form class="mt-9 grid gap-5" onsubmit={login}>
			<div>
				<label class="mb-2 block font-bold" for="email">Email</label>
				<input class="w-full rounded-md border border-line bg-[#faf7ef] px-3.5 py-3 font-[inherit] focus-visible:outline-3 focus-visible:outline-offset-2 focus-visible:outline-clay" id="email" name="email" type="email" autocomplete="email" maxlength="254" required />
			</div>
			<div>
				<label class="mb-2 block font-bold" for="password">Password</label>
				<input class="w-full rounded-md border border-line bg-[#faf7ef] px-3.5 py-3 font-[inherit] focus-visible:outline-3 focus-visible:outline-offset-2 focus-visible:outline-clay" id="password" name="password" type="password" autocomplete="current-password" maxlength="128" required />
			</div>

			<button class="w-full cursor-pointer rounded-md border border-forest bg-forest px-4 py-3.5 font-[inherit] font-bold text-cream hover:bg-forest-dark focus-visible:outline-3 focus-visible:outline-offset-3 focus-visible:outline-clay disabled:cursor-wait disabled:opacity-70" type="submit" disabled={submitting}>{submitting ? 'Signing in…' : 'Sign in'}</button>
			{#if message}
				<p class="m-0 leading-relaxed text-danger" role="alert" aria-live="polite">{message}</p>
			{/if}
		</form>
	</section>
</main>
