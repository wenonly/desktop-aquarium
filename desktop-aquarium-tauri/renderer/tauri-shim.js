// Tauri 适配层：把 Tauri 事件/命令映射成与 Electron preload 相同的 window.aquarium API。
// 在 Electron（preload 已注入 window.aquarium）或纯浏览器环境下自动失效。
(function () {
  if (window.aquarium || !window.__TAURI__) return;
  const listen = window.__TAURI__.event.listen;
  const invoke = window.__TAURI__.core.invoke;
  const actionCbs = [];
  const modeCbs = [];
  const cursorCbs = [];
  listen("action", (e) => actionCbs.forEach((cb) => cb(e.payload)));
  listen("mode-changed", (e) => modeCbs.forEach((cb) => cb(e.payload)));
  listen("cursor", (e) => cursorCbs.forEach((cb) => cb(e.payload)));
  window.aquarium = {
    exitInteractive: () => invoke("exit_interactive"),
    reportSize: (w, h, search) => invoke("report_size", { w, h, search }),
    reportStats: (s) => invoke("report_stats", s),
    onAction: (cb) => actionCbs.push(cb),
    onModeChanged: (cb) => modeCbs.push(cb),
    onCursor: (cb) => cursorCbs.push(cb)
  };
})();
