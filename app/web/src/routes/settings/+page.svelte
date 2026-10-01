<script lang="ts">
	import LogoutButton from '$lib/LogoutButton.svelte';
</script>

<svelte:head><title>Settings · BookReplay</title></svelte:head>

<main id="main-content" class="mx-auto min-h-screen w-[calc(100%-2rem)] max-w-5xl px-4 pt-12 pb-20">
	<p class="mb-3 text-xs font-extrabold tracking-[0.14em] text-clay uppercase">Personalize BookReplay</p>
	<h1 class="font-serif text-5xl tracking-tight">Settings</h1>
	<section class="mt-10" aria-labelledby="account-title">
		<h2 id="account-title" class="font-serif text-2xl">Account</h2>
		<a href="/password/" class="mt-5 inline-block rounded-md bg-forest px-5 py-3 font-bold text-cream no-underline hover:bg-forest-dark focus-visible:outline-3 focus-visible:outline-offset-2 focus-visible:outline-clay">Change password</a>
		<div class="mt-6"><LogoutButton /></div>
	</section>
</main>
