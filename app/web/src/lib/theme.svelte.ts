export const theme = $state({ preference: 'system', ready: false });
const storageKey = 'bookreplay-theme';

function readPreference() {
	let stored: string | null = null;
	try { stored = localStorage.getItem(storageKey); } catch {}
	return stored === 'dark' || stored === 'light' ? stored : 'system';
}

function applyTheme() {
	const dark = theme.preference === 'dark' ||
		(theme.preference === 'system' && matchMedia('(prefers-color-scheme: dark)').matches);
	document.documentElement.dataset.theme = dark ? 'dark' : 'light';
	document.querySelector('meta[name="theme-color"]')?.setAttribute('content', dark ? '#29261f' : '#f5f1e8');
}

export function changeTheme() {
	try { localStorage.setItem(storageKey, theme.preference); } catch {}
	applyTheme();
}

// Keep system and cross-tab updates active after navigating away from Settings.
export function initializeTheme() {
	theme.preference = readPreference();
	applyTheme();
	theme.ready = true;
	const system = matchMedia('(prefers-color-scheme: dark)');
	const syncPreference = (event: StorageEvent) => {
		if (event.key === storageKey || event.key === null) {
			theme.preference = readPreference();
			applyTheme();
		}
	};
	system.addEventListener('change', applyTheme);
	window.addEventListener('storage', syncPreference);
	return () => {
		system.removeEventListener('change', applyTheme);
		window.removeEventListener('storage', syncPreference);
	};
}
