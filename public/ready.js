// Shows the window as soon as the static frame in index.html has painted,
// without waiting for the UI bundle. The app fills in moments later.
(function () {
  var done = false;
  function ready() {
    if (done) return;
    done = true;
    var ipc = window.__TAURI_INTERNALS__;
    if (ipc) ipc.invoke("app_ready");
  }
  requestAnimationFrame(function () {
    requestAnimationFrame(ready);
  });
  setTimeout(ready, 120);
})();
