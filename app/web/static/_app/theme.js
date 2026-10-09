// Run before paint; an external script also respects the app's script-src policy.
(() => {
	let theme = 'system';
	try { theme = localStorage.getItem('bookreplay-theme') || 'system'; } catch {}
	const dark = theme === 'dark' ||
		(theme !== 'light' && matchMedia('(prefers-color-scheme: dark)').matches);
	document.documentElement.dataset.theme = dark ? 'dark' : 'light';
	document.querySelector('meta[name="theme-color"]').content = dark ? '#29261f' : '#f5f1e8';
})();
