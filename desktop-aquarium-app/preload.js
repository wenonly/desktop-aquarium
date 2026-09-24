"use strict";
const { contextBridge, ipcRenderer } = require("electron");

contextBridge.exposeInMainWorld("aquarium", {
  exitInteractive: () => ipcRenderer.send("exit-interactive"),
  onAction: (cb) => ipcRenderer.on("action", (_e, a) => cb(a)),
  onModeChanged: (cb) => ipcRenderer.on("mode-changed", (_e, m) => cb(m))
});
