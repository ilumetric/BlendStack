// Runs before the UI bundle so the very first frame already has the right
// theme and window material (hints are injected by the Rust side).
(function () {
  var hints = window.__BLENDSTACK__ || {};
  var root = document.documentElement;
  if (hints.translucent) root.classList.add("translucent");
  if (hints.os) root.classList.add("os-" + hints.os);
  if (hints.theme === "light" || hints.theme === "dark") root.dataset.theme = hints.theme;
})();
