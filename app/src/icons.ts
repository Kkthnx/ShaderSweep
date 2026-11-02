const svg = (body: string, size = 20) =>
  `<svg width="${size}" height="${size}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${body}</svg>`;

export const icons = {
  logo: `<svg width="24" height="24" viewBox="0 0 24 24" aria-hidden="true"><path d="M10.5 3 12.4 9.6 19 11.5 12.4 13.4 10.5 20 8.6 13.4 2 11.5 8.6 9.6Z" fill="currentColor"/><path d="M19 3.5 19.8 6.2 22.5 7 19.8 7.8 19 10.5 18.2 7.8 15.5 7 18.2 6.2Z" fill="currentColor" opacity=".6"/></svg>`,
  refresh: svg(`<path d="M20 11a8 8 0 1 0-2.3 5.7"/><path d="M20 4v7h-7"/>`),
  chevron: svg(`<path d="m9 6 6 6-6 6"/>`, 16),
  check: svg(`<path d="m5 12.5 4.5 4.5L19 7.5"/>`, 16),
  info: svg(`<circle cx="12" cy="12" r="9"/><path d="M12 11v5"/><path d="M12 7.6v.1"/>`, 18),
  github: svg(
    `<path d="M15 22v-4a4.8 4.8 0 0 0-1-3.5c3 0 6-2 6-5.5.08-1.25-.27-2.48-1-3.5.28-1.15.28-2.35 0-3.5 0 0-1 0-3 1.5-2.64-.5-5.36-.5-8 0C6 2 5 2 5 2c-.3 1.15-.3 2.35 0 3.5A5.4 5.4 0 0 0 4 9c0 3.5 3 5.5 6 5.5-.39.49-.68 1.05-.85 1.65-.17.6-.22 1.23-.15 1.85v4"/><path d="M9 18c-4.51 2-5-2-7-2"/>`,
    15,
  ),
  drive: svg(
    `<path d="M22 12H2"/><path d="M5.45 5.11 2 12v6a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-6l-3.45-6.89A2 2 0 0 0 16.76 4H7.24a2 2 0 0 0-1.79 1.11z"/><path d="M6 16h.01"/><path d="M10 16h.01"/>`,
    14,
  ),
  globe: svg(`<circle cx="12" cy="12" r="9"/><path d="M3 12h18"/><path d="M12 3a14 14 0 0 1 0 18 14 14 0 0 1 0-18"/>`, 15),
  code: svg(`<path d="m8 7-5 5 5 5"/><path d="m16 7 5 5-5 5"/>`, 15),
  flag: svg(`<path d="M5 21V4"/><path d="M5 4h12l-2 4 2 4H5"/>`, 15),
  download: svg(`<path d="M12 4v10"/><path d="m7.5 10 4.5 4.5 4.5-4.5"/><path d="M5 19h14"/>`, 15),
  alert: svg(`<path d="M12 4 2.8 19.5h18.4Z"/><path d="M12 10v4.5"/><path d="M12 17.2v.1"/>`, 18),
};
