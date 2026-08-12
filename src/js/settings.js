// Customization window: background selector + theme controls. Everything
// writes through set_theme so config.toml stays the single source of truth.

import { initThemeControls } from "./theme-controls.js";

const tauri = window.__TAURI__;

function invoke(command, args) {
  if (!tauri) return Promise.resolve();
  return tauri.core.invoke(command, args).catch((e) => console.error(command, e));
}

window.addEventListener("error", (e) =>
  invoke("frontend_log", { message: `${e.message} (${e.filename}:${e.lineno})` })
);
window.addEventListener("unhandledrejection", (e) =>
  invoke("frontend_log", { message: String(e.reason) })
);

function assetUrl(path) {
  return tauri ? tauri.core.convertFileSrc(path) : path;
}

function el(tag, className, text) {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}

const themeSync = initThemeControls(document.getElementById("theme-controls"), invoke);

let configDir = "";
let currentBackground = "";
let options = [];

function renderBackgrounds() {
  const grid = document.getElementById("background-grid");
  grid.replaceChildren();

  const gradient = el("button", "bg-tile");
  gradient.type = "button";
  gradient.dataset.path = "";
  gradient.append(el("div", "bg-preview gradient-preview"), el("span", "bg-name", "Gradient"));
  gradient.addEventListener("click", () => choose(""));
  grid.append(gradient);

  for (const option of options) {
    const tile = el("button", "bg-tile");
    tile.type = "button";
    tile.dataset.path = option.path;
    let preview;
    if (option.kind === "video") {
      preview = document.createElement("video");
      preview.muted = true;
      preview.preload = "metadata"; // first frame as thumbnail, no playback
      preview.src = assetUrl(option.path);
    } else {
      preview = document.createElement("img");
      preview.loading = "lazy";
      preview.alt = "";
      preview.src = assetUrl(option.path);
    }
    preview.className = "bg-preview";
    tile.append(preview, el("span", "bg-name", option.name));
    tile.addEventListener("click", () => choose(option.path));
    grid.append(tile);
  }
  highlight();
}

function choose(path) {
  currentBackground = path;
  highlight();
  invoke("set_theme", { key: "background", value: path });
}

function highlight() {
  for (const tile of document.querySelectorAll(".bg-tile")) {
    tile.classList.toggle("active", tile.dataset.path === currentBackground);
  }
}

document.getElementById("open-config").addEventListener("click", () => {
  if (configDir) invoke("open_shortcut", { uri: configDir });
});

if (tauri) {
  tauri.event.listen("config-update", (event) => {
    const state = event.payload;
    for (const [name, value] of state.tokens) {
      document.documentElement.style.setProperty(name, value);
    }
    themeSync(Object.fromEntries(state.tokens));
    configDir = state.config_dir;
    currentBackground = state.background.path || "";
    highlight();
  });
  invoke("frontend_ready");
  invoke("list_backgrounds").then((list) => {
    options = list || [];
    renderBackgrounds();
  });
}
