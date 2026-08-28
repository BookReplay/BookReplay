<script lang="ts">
	let loggingOut = $state(false);

	async function logout(event: SubmitEvent) {
		event.preventDefault();
		loggingOut = true;

		try {
			const response = await fetch('/api/auth/logout', { method: 'POST' });
			if (response.ok || response.status === 401) window.location.assign('/login/');
		} finally {
			loggingOut = false;
		}
	}
</script>

<form action="/api/auth/logout" method="post" onsubmit={logout}>
	<button class="cursor-pointer border-0 bg-transparent p-0 font-[inherit] text-sm font-bold text-muted underline decoration-line underline-offset-4 hover:text-clay focus-visible:outline-3 focus-visible:outline-offset-4 focus-visible:outline-clay disabled:cursor-wait disabled:opacity-60" type="submit" disabled={loggingOut}>{loggingOut ? 'Signing out…' : 'Sign out'}</button>
</form>
