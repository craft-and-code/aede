(() => {
  const language = document.documentElement.lang === 'en' ? 'en' : 'fr';
  let saved;
  try { saved = localStorage.getItem('aede-language'); } catch {}
  const explicitEnglish = language === 'en';
  if (!explicitEnglish && (saved === 'en' || (!saved && !navigator.language.toLowerCase().startsWith('fr')))) {
    // Use the current origin so previews and project-hosted URLs both work.
    const current = new URL(location.href);
    const prefix = current.pathname.replace(/(?:index\.html)?$/, '');
    location.replace(new URL(prefix + 'en/' + current.search + current.hash, current.origin).href);
  }
  document.addEventListener('click', event => {
    const link = event.target.closest('[data-lang]');
    if (link) try { localStorage.setItem('aede-language', link.dataset.lang); } catch {}
  });
})();
