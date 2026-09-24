"use strict";
// 桌面鱼塘 · Electron 主进程
// 透明、无边框、置顶、鼠标穿透的全屏覆盖窗口；托盘菜单 + 全局快捷键交互。
const { app, BrowserWindow, Tray, Menu, globalShortcut, screen, ipcMain, nativeImage } = require("electron");
const path = require("path");

let win = null;
let tray = null;
let interactive = false;
let paused = false;
let origin = { x: 0, y: 0 };
let resizeTimer = null;

// 单实例
if (!app.requestSingleInstanceLock()) {
  app.quit();
}

// 所有显示器的联合边界（支持跨屏游动）
function unionBounds() {
  const ds = screen.getAllDisplays();
  let x1 = Infinity, y1 = Infinity, x2 = -Infinity, y2 = -Infinity;
  for (const d of ds) {
    x1 = Math.min(x1, d.bounds.x);
    y1 = Math.min(y1, d.bounds.y);
    x2 = Math.max(x2, d.bounds.x + d.bounds.width);
    y2 = Math.max(y2, d.bounds.y + d.bounds.height);
  }
  return { x: x1, y: y1, width: x2 - x1, height: y2 - y1 };
}

function send(action) {
  if (win) win.webContents.send("action", action);
}

// 鼠标屏幕坐标 -> 渲染层窗口内坐标
function cursorLocal() {
  const p = screen.getCursorScreenPoint();
  return { x: Math.round(p.x - origin.x), y: Math.round(p.y - origin.y) };
}

function setInteractive(v) {
  interactive = v;
  if (!win) return;
  if (v) {
    win.setIgnoreMouseEvents(false);
    try { win.focus(); } catch (e) { /* macOS 隐藏 dock 时可能失败 */ }
  } else {
    win.setIgnoreMouseEvents(true, { forward: true });
  }
  win.webContents.send("mode-changed", { interactive: v });
  updateTray();
}

function togglePause() {
  paused = !paused;
  send({ type: "pause" });
  updateTray();
}

function createWindow() {
  const b = unionBounds();
  origin = { x: b.x, y: b.y };
  win = new BrowserWindow({
    x: b.x, y: b.y, width: b.width, height: b.height,
    frame: false,
    transparent: true,
    resizable: false,
    movable: false,
    minimizable: false,
    maximizable: false,
    fullscreenable: false,
    skipTaskbar: true,
    hasShadow: false,
    enableLargerThanScreen: true, // 允许窗口大于单屏（多显示器联合）
    show: false,
    webPreferences: {
      preload: path.join(__dirname, "preload.js"),
      contextIsolation: true,
      nodeIntegration: false
    }
  });
  win.setAlwaysOnTop(true, "screen-saver");
  win.setVisibleOnAllWorkspaces(true, { visibleOnFullScreen: true });
  // 默认鼠标穿透：不影响桌面任何操作
  win.setIgnoreMouseEvents(true, { forward: true });
  win.loadFile(path.join(__dirname, "renderer", "index.html"));
  win.once("ready-to-show", () => win.showInactive());
  win.on("closed", () => { win = null; });
}

function createTray() {
  const icon = nativeImage.createFromPath(path.join(__dirname, "assets", "tray-icon.png"));
  tray = new Tray(icon);
  tray.setToolTip("桌面鱼塘");
  updateTray();
}

function updateTray() {
  if (!tray) return;
  const menu = Menu.buildFromTemplate([
    {
      label: interactive ? "退出交互模式（Ctrl+Alt+F）" : "进入交互模式（Ctrl+Alt+F）",
      click: () => setInteractive(!interactive)
    },
    { label: "撒鱼食（Ctrl+Alt+D）", click: () => send(Object.assign({ type: "food" }, cursorLocal())) },
    { label: "召唤鲨鱼（Ctrl+Alt+X）", click: () => send(Object.assign({ type: "shark" }, cursorLocal())) },
    { label: paused ? "继续" : "暂停", click: togglePause },
    { type: "separator" },
    { label: "退出", click: () => app.quit() }
  ]);
  tray.setContextMenu(menu);
}

function registerShortcuts() {
  globalShortcut.register("CmdOrCtrl+Alt+F", () => setInteractive(!interactive));
  globalShortcut.register("CmdOrCtrl+Alt+D", () => send(Object.assign({ type: "food" }, cursorLocal())));
  globalShortcut.register("CmdOrCtrl+Alt+X", () => send(Object.assign({ type: "shark" }, cursorLocal())));
  globalShortcut.register("CmdOrCtrl+Alt+P", togglePause);
}

function onDisplaysChanged() {
  clearTimeout(resizeTimer);
  resizeTimer = setTimeout(() => {
    if (!win) return;
    const b = unionBounds();
    origin = { x: b.x, y: b.y };
    win.setBounds(b);
  }, 300);
}

ipcMain.on("exit-interactive", () => setInteractive(false));

app.whenReady().then(() => {
  if (process.platform === "darwin" && app.dock) app.dock.hide();
  createWindow();
  createTray();
  registerShortcuts();
  screen.on("display-added", onDisplaysChanged);
  screen.on("display-removed", onDisplaysChanged);
  screen.on("display-metrics-changed", onDisplaysChanged);
});

// 常驻托盘：所有窗口关闭时不退出
app.on("window-all-closed", (e) => e.preventDefault());
app.on("will-quit", () => globalShortcut.unregisterAll());
