import { writable } from 'svelte/store';

export type Theme = 'dark' | 'light';

const THEME_KEY = 'nexus-theme';

function getInitial(): Theme {
  if (typeof localStorage !== 'undefined') {
    const stored = localStorage.getItem(THEME_KEY);
    if (stored === 'light' || stored === 'dark') return stored;
  }
  return 'dark';
}

export const theme = writable<Theme>(getInitial());

theme.subscribe((val) => {
  if (typeof localStorage !== 'undefined') {
    localStorage.setItem(THEME_KEY, val);
  }
  if (typeof document !== 'undefined') {
    document.documentElement.setAttribute('data-theme', val);
  }
});

export function toggleTheme() {
  theme.update(t => t === 'dark' ? 'light' : 'dark');
}
