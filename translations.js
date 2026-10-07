(() => {
  const translations = window.aedeHomeTranslations || { en: {} };
  const dict = document.documentElement.lang === 'en' ? translations.en : {};
  window.aedeTranslate = value => dict[value.trim()] ?? value;
  window.aedeLocalize = root => {
    if (!root || document.documentElement.lang !== 'en') return;
    const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT, {acceptNode: node => node.parentElement?.closest('script,style') ? NodeFilter.FILTER_REJECT : NodeFilter.FILTER_ACCEPT});
    while (walker.nextNode()) {const node=walker.currentNode,trimmed=node.data.trim(),replacement=dict[trimmed];if(replacement!==undefined && replacement!==trimmed)node.data=node.data.replace(trimmed,replacement);}
    for(const element of [root,...root.querySelectorAll('[aria-label],[alt],[title],[placeholder]')])for(const key of ['aria-label','alt','title','placeholder']){const value=element.getAttribute(key);if(value && dict[value.trim()]!==undefined)element.setAttribute(key,dict[value.trim()]);}
  };
})();
