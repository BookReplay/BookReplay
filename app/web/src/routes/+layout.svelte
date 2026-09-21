<script lang="ts">
	import favicon from '$lib/assets/favicon.svg';
	import '../app.css';

	let { children } = $props();
</script>

<svelte:head>
	<link rel="icon" href={favicon} />
	<meta name="theme-color" content="#f5f1e8" />
</svelte:head>

<div class="flex min-h-dvh flex-col">
	{@render children()}

	<footer class="mx-auto flex w-[calc(100%-2rem)] max-w-6xl flex-wrap items-center justify-between gap-x-6 gap-y-2 border-t border-line py-6 text-sm text-muted max-sm:w-[calc(100%-1.5rem)]">
		<a href="/" class="rounded-sm font-serif text-xl tracking-tight text-ink focus-visible:outline-3 focus-visible:outline-offset-4 focus-visible:outline-clay">BookReplay</a>
		<p>Keep the words that stay with you.</p>
	</footer>
</div>

<style>
	:global(#main-content) {
		min-height: 0;
		flex: 1;
	}
</style>
