/* Static pages deliberately preserve navigation state across full-page links. */
(() => {
  "use strict";
  const french = document.documentElement.lang === "fr";
  const sidebar = document.querySelector(".guide-sidebar");
  const menu = document.querySelector(".guide-menu");
  const groups = sidebar ? [...sidebar.querySelectorAll("[data-nav-section]")] : [];
  const input = sidebar?.querySelector('input[type="search"]');
  const scrollKey = "aede-docs-sidebar-scroll-v1";
  const groupKey = "aede-docs-sidebar-groups-v1";
  const filterKey = "aede-docs-sidebar-filter-v1";
  let filtering = false, restoring = false, preSearchScroll = 0;
  const storage = {
    get(key) { try { return sessionStorage.getItem(key); } catch { return null; } },
    set(key, value) { try { sessionStorage.setItem(key, value); } catch { /* Navigation still works when storage is unavailable. */ } },
  };
  let filterState = {}, savedGroups = {};
  try { savedGroups = JSON.parse(storage.get(groupKey) || "{}"); } catch { /* Ignore a stale state value. */ }
  try { filterState = JSON.parse(storage.get(filterKey) || "{}"); } catch { /* Ignore a stale search value. */ }
  const savedScroll = Number(storage.get(scrollKey));
  const previousLocale = storage.get("aede-docs-sidebar-locale-v1");
  const currentLocale = french ? "fr" : "en";
  const mayRevealCurrent = previousLocale !== currentLocale || storage.get(scrollKey) === null;
  storage.set("aede-docs-sidebar-locale-v1", currentLocale);
  preSearchScroll = Number.isFinite(filterState.before) ? filterState.before : savedScroll;
  const saveFilter = () => storage.set(filterKey, filtering ? JSON.stringify({query: input.value, scroll: sidebar.scrollTop, before: preSearchScroll}) : "");
  const saveScroll = () => {
    // A hidden mobile menu has no scroll range; do not overwrite the position
    // saved by the visible menu just before navigation.
    if (!sidebar || restoring || !sidebar.getBoundingClientRect().height) return;
    if (filtering) saveFilter(); else storage.set(scrollKey, String(sidebar.scrollTop));
  };
  const saveGroups = () => {
    if (!filtering) storage.set(groupKey, JSON.stringify(Object.fromEntries(groups.map(group => [group.dataset.navSection, group.open]))));
  };
  const restoreScroll = (value, revealCurrent = false) => {
    if (!sidebar || !Number.isFinite(value)) return;
    restoring = true;
    requestAnimationFrame(() => {
      sidebar.scrollTop = value;
      requestAnimationFrame(() => {
        sidebar.scrollTop = value;
        if (revealCurrent && sidebar.getBoundingClientRect().height) {
          const active = sidebar.querySelector('[aria-current="page"]');
          if (active && !active.closest("li")?.hidden) {
            const box = sidebar.getBoundingClientRect(), item = active.getBoundingClientRect();
            const safeTop = box.top + (input?.getBoundingClientRect().height || 0) + 14;
            if (item.top < safeTop) sidebar.scrollTop += item.top - safeTop;
            else if (item.bottom > box.bottom - 14) sidebar.scrollTop += item.bottom - box.bottom + 14;
          }
        }
        restoring = false;
      });
    });
  };
  for (const group of groups) {
    if (typeof savedGroups[group.dataset.navSection] === "boolean") group.open = savedGroups[group.dataset.navSection];
    if (group.querySelector('[aria-current="page"]')) group.open = true;
  }
  const normalise = text => text.toLowerCase().normalize("NFD").replace(/[\u0300-\u036f]/g, "");
  const applySearch = (initial = false) => {
    const terms = normalise(input.value).trim().split(/\s+/).filter(Boolean);
    const wasFiltering = filtering;
    if (terms.length && !filtering && !initial) {
      preSearchScroll = sidebar.scrollTop;
      storage.set(scrollKey, String(preSearchScroll));
      saveGroups(); savedGroups = Object.fromEntries(groups.map(group => [group.dataset.navSection, group.open]));
    }
    filtering = terms.length > 0;
    let count = 0;
    for (const item of sidebar.querySelectorAll("[data-nav-item]")) {
      item.hidden = !terms.every(term => normalise(item.textContent).includes(term));
      if (!item.hidden) count++;
    }
    for (const bucket of sidebar.querySelectorAll("[data-nav-bucket]")) bucket.hidden = !bucket.querySelector("[data-nav-item]:not([hidden])");
    for (const group of groups) {
      group.hidden = !group.querySelector("[data-nav-item]:not([hidden])");
      group.open = filtering ? !group.hidden : Boolean(savedGroups[group.dataset.navSection] ?? group.querySelector('[aria-current="page"]'));
    }
    sidebar.querySelector(".guide-search-status").textContent = terms.length ? (french ? `${count} résultat${count === 1 ? "" : "s"}` : `${count} result${count === 1 ? "" : "s"}`) : "";
    if (wasFiltering && !filtering) restoreScroll(preSearchScroll);
    saveFilter();
  };
  if (input && filterState.query) { input.value = filterState.query; applySearch(true); }
  restoreScroll(filtering ? Number(filterState.scroll) || 0 : savedScroll, mayRevealCurrent);
  // Register toggle handling after initial restoration has settled.
  requestAnimationFrame(() => groups.forEach(group => group.addEventListener("toggle", saveGroups)));
  input?.addEventListener("input", () => applySearch());
  sidebar?.addEventListener("scroll", saveScroll, { passive: true });
  sidebar?.addEventListener("click", event => { if (event.target.closest("a[href]")) { saveScroll(); saveGroups(); } });
  window.addEventListener("pagehide", () => { saveScroll(); saveGroups(); });
  const openMenu = open => {
    if (!sidebar || !menu) return;
    sidebar.classList.toggle("is-open", open);
    menu.setAttribute("aria-expanded", String(open));
    if (open) {
      input?.focus({preventScroll: true});
      let filter = {};
      try { filter = JSON.parse(storage.get(filterKey) || "{}"); } catch { /* No saved search. */ }
      restoreScroll(filtering ? Number(filter.scroll) || 0 : Number(storage.get(scrollKey)), mayRevealCurrent);
    }
  };
  menu?.addEventListener("click", () => openMenu(menu.getAttribute("aria-expanded") !== "true"));
  document.addEventListener("keydown", event => { if (event.key === "Escape" && menu?.getAttribute("aria-expanded") === "true") { openMenu(false); menu.focus(); } });
  document.addEventListener("click", event => { if (menu && sidebar && !menu.contains(event.target) && !sidebar.contains(event.target)) openMenu(false); });
  for (const link of document.querySelectorAll("[data-language]")) {
    // Corresponding translated headings have different fragment names. The
    // build supplies a map when both sources share the same heading outline.
    link.addEventListener("click", () => {
      try { localStorage.setItem("aede-language", link.dataset.language); } catch { /* Explicit link remains usable. */ }
      if (location.hash && !link.hash) {
        let identifier;
        try { identifier = decodeURIComponent(location.hash.slice(1)); } catch { identifier = location.hash.slice(1); }
        let mapping = {};
        try { mapping = JSON.parse(link.dataset.fragmentMap || "{}"); } catch { /* Preserve the original fragment when no mapping exists. */ }
        link.hash = mapping[identifier] || identifier;
      }
    });
  }
  const revealFragment = () => {
    let identifier;
    try { identifier = decodeURIComponent(location.hash.slice(1)); } catch { return; }
    const target = document.getElementById(identifier);
    if (!target) return;
    let ancestor = target.parentElement;
    while (ancestor) { if (ancestor.tagName === "DETAILS") ancestor.open = true; ancestor = ancestor.parentElement; }
    requestAnimationFrame(() => target.scrollIntoView({ block: "start" }));
  };
  window.addEventListener("hashchange", revealFragment);
  if (location.hash) revealFragment();
  for (const pre of document.querySelectorAll(".guide-prose pre")) {
    const code = pre.querySelector("code");
    if (!code) continue;
    const language = [...code.classList].find(name => name.startsWith("language-"))?.slice(9);
    if (["sh", "bash", "zsh", "shell", "powershell"].includes(language)) {
      pre.classList.add("terminal-code-block");
      const header = document.createElement("div"); header.className = "terminal-code-header";
      const controls = document.createElement("span"); controls.className = "terminal-code-controls"; controls.setAttribute("aria-hidden", "true");
      for (let i = 0; i < 3; i++) controls.append(document.createElement("i"));
      const title = document.createElement("span"); title.className = "terminal-code-title"; title.textContent = language === "powershell" ? "PowerShell" : "Terminal";
      header.append(controls, title); pre.prepend(header);
    }
    if (!navigator.clipboard?.writeText) continue;
    const button = document.createElement("button"); button.type = "button"; button.className = "copy-code"; button.textContent = french ? "Copier" : "Copy";
    button.addEventListener("click", async () => {
      try { await navigator.clipboard.writeText(code.textContent.replace(/\r?\n$/, "")); button.textContent = french ? "Copié ✓" : "Copied ✓"; }
      catch { button.textContent = french ? "Sélectionnez le texte" : "Select the text"; }
      setTimeout(() => { button.textContent = french ? "Copier" : "Copy"; }, 1800);
    });
    pre.prepend(button);
  }
})();
