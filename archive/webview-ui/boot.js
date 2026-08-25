/* Mark desktop shell ASAP — avoid mock padding flash under native chrome.
   External (not inline) so the CSP can drop script-src 'unsafe-inline';
   must stay in <head> before app.css so the first paint already has .desktop. */
(function () {
  try {
    var ua = navigator.userAgent || '';
    var host = location.hostname || '';
    var tauri =
      !!(window.__TAURI_INTERNALS__ || window.__TAURI__) ||
      ua.indexOf('Tauri') !== -1 ||
      location.protocol === 'tauri:' ||
      host === 'tauri.localhost' ||
      host.endsWith('.localhost') && host.indexOf('tauri') !== -1;
    if (tauri) document.documentElement.classList.add('desktop');
  } catch (_) {}
})();
