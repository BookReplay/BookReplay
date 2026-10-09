<script lang="ts">
	import { theme, changeTheme } from '$lib/theme.svelte';
</script>

<label class="flex items-center gap-2">
	<span>Theme</span>
	<select class="min-h-11 cursor-pointer rounded-md border border-line bg-cream px-3 py-2 text-ink" bind:value={theme.preference} onchange={changeTheme} disabled={!theme.ready}>
		<option value="system">System</option>
		<option value="light">Light</option>
		<option value="dark">Dark</option>
	</select>
</label>
