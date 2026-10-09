<script lang="ts">
	import { onMount } from 'svelte';
	import icon from '$lib/assets/icon.png';
	import Sidebar from '$lib/Sidebar.svelte';
	import { initializeTheme } from '$lib/theme.svelte';
	import { page } from '$app/state';
	import '../app.css';

	let { children } = $props();
	let showSidebar = $derived(!['/login/', '/register/', '/password/'].includes(page.url.pathname));
	let version = $state('');

	onMount(initializeTheme);

	onMount(async () => {
		try {
			const response = await fetch('/api/version');
			if (response.ok) version = await response.text();
		} catch {}
	});
</script>

<svelte:head>
	<link rel="icon" type="image/png" href={icon} />
	<link rel="apple-touch-icon" href={icon} />
</svelte:head>

<div class="flex min-h-dvh flex-col lg:flex-row">
	{#if showSidebar}
		<header class="app-header">
			<a class="brand" href="/"><img class="brand-mark" src={icon} alt="" />BookReplay</a>
			<span class="header-note">A home for the ideas you keep.</span>
			<a class="import-link" href="/import/"><span aria-hidden="true">＋</span> Import highlights</a>
		</header>
		<Sidebar />
	{/if}
	<div class:with-shell={showSidebar} class="flex min-w-0 flex-1 flex-col">
		{@render children()}

		<footer class="mx-auto flex w-[calc(100%-2rem)] max-w-6xl flex-wrap items-center justify-between gap-x-6 gap-y-2 border-t border-line py-6 text-sm text-muted max-sm:w-[calc(100%-1.5rem)]">
			<p class="m-0">© {new Date().getFullYear()} BookReplay</p>
			<a class="text-link underline-offset-4 hover:underline" href="https://bookreplay.com" target="_blank" rel="noreferrer">bookreplay.com</a>
			{#if version}<span>Version {version}</span>{/if}
		</footer>
	</div>
</div>

<style>
	:global(#main-content) {
		min-height: 0;
		flex: 1;
	}
</style>
