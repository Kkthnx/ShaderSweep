const svg = (body: string, size = 20) =>
  `<svg width="${size}" height="${size}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${body}</svg>`;

export const icons = {
  logo: `<svg width="24" height="24" viewBox="0 0 24 24" aria-hidden="true"><path d="M10.5 3 12.4 9.6 19 11.5 12.4 13.4 10.5 20 8.6 13.4 2 11.5 8.6 9.6Z" fill="currentColor"/><path d="M19 3.5 19.8 6.2 22.5 7 19.8 7.8 19 10.5 18.2 7.8 15.5 7 18.2 6.2Z" fill="currentColor" opacity=".6"/></svg>`,
  refresh: svg(`<path d="M20 11a8 8 0 1 0-2.3 5.7"/><path d="M20 4v7h-7"/>`),
  chevron: svg(`<path d="m9 6 6 6-6 6"/>`, 16),
  check: svg(`<path d="m5 12.5 4.5 4.5L19 7.5"/>`, 16),
  info: svg(`<circle cx="12" cy="12" r="9"/><path d="M12 11v5"/><path d="M12 7.6v.1"/>`, 18),
  alert: svg(`<path d="M12 4 2.8 19.5h18.4Z"/><path d="M12 10v4.5"/><path d="M12 17.2v.1"/>`, 18),
};
