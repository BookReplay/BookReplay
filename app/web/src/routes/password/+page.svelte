<script lang="ts">
	let message = $state('');
	let submitting = $state(false);
	async function changePassword(event: SubmitEvent) {
		event.preventDefault();
		const form = event.currentTarget as HTMLFormElement;
		const data = new FormData(form);
		if (data.get('new_password') !== data.get('confirmation')) {
			message = 'New passwords do not match.';
			return;
		}
		submitting = true;
		try {
			const response = await fetch('/api/auth/password', {
				method: 'POST', headers: { 'content-type': 'application/json' },
				body: JSON.stringify({ current_password: data.get('current_password'), new_password: data.get('new_password') })
			});
			if (response.ok) {
				form.reset();
				window.location.assign('/login/');
			} else {
				const result = await response.json().catch(() => ({}));
				message = result.error ?? 'Password change failed. Wait one minute and try again.';
			}
		} catch { message = 'Could not reach BookReplay. Try again.'; }
		finally { submitting = false; }
	}
</script>

<svelte:head><title>Change password · BookReplay</title></svelte:head>
<main id="main-content" class="mx-auto w-[calc(100%-2rem)] max-w-lg py-12">
	<a href="/" class="underline">Back to library</a>
	<h1 class="my-6 font-serif text-4xl">Change password</h1>
	<p>Use 12–128 bytes. Changing your password signs out every session, including this one.</p>
	<form class="mt-6 grid gap-4" onsubmit={changePassword}>
		<label for="current-password">Current password</label>
		<input class="rounded-md border border-line p-3" id="current-password" name="current_password" type="password" autocomplete="current-password" maxlength="128" required />
		<label for="new-password">New password</label>
		<input class="rounded-md border border-line p-3" id="new-password" name="new_password" type="password" autocomplete="new-password" minlength="12" maxlength="128" required />
		<label for="confirmation">Confirm new password</label>
		<input class="rounded-md border border-line p-3" id="confirmation" name="confirmation" type="password" autocomplete="new-password" minlength="12" maxlength="128" required />
		<button class="rounded-md bg-forest p-3 font-bold text-cream disabled:opacity-70" disabled={submitting}>{submitting ? 'Changing…' : 'Change password'}</button>
		{#if message}<p role="alert" class="text-danger">{message}</p>{/if}
	</form>
</main>
