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

/* ---------- widgets ---------- */

const WIDGET_KINDS = ["clock", "media", "weather", "shortcut", "stats", "text"];

function widgetSummary(widget) {
  switch (widget.kind) {
    case "clock":
      return widget.format === "24h" ? "24h" : "12h";
    case "media":
      return "now playing";
    case "weather":
      return widget.label || `${widget.lat}, ${widget.lon}`;
    case "shortcut":
      return widget.label || widget.uri;
    case "stats":
      return ["cpu", "ram", "disk"]
        .filter((m) => widget[`show_${m}`])
        .join(" · ");
    case "text":
      return `“${widget.text}”`;
    default:
      return "";
  }
}

function renderWidgets(widgets) {
  const list = document.getElementById("widget-list");
  list.replaceChildren();
  widgets.forEach((widget, index) => {
    const row = el("div", "widget-row");
    row.append(
      el("span", "widget-kind", widget.kind),
      el("span", "widget-summary dim", widgetSummary(widget))
    );
    const actions = el("div", "widget-actions");
    const button = (label, title, disabled, onClick) => {
      const b = el("button", "widget-action", label);
      b.type = "button";
      b.title = title;
      b.disabled = disabled;
      b.addEventListener("click", onClick);
      actions.append(b);
    };
    button("↑", "Move up", index === 0, () =>
      invoke("widget_move", { index, up: true })
    );
    button("↓", "Move down", index === widgets.length - 1, () =>
      invoke("widget_move", { index, up: false })
    );
    button("✕", "Remove", false, () => invoke("widget_remove", { index }));
    row.append(actions);
    list.append(row);
  });
  if (!widgets.length) {
    list.append(el("div", "widget-summary dim", "No widgets — add one below."));
  }
}

const addRow = document.getElementById("widget-add");
for (const kind of WIDGET_KINDS) {
  const b = el("button", "widget-add-button", `+ ${kind}`);
  b.type = "button";
  b.addEventListener("click", () => invoke("widget_add", { kind }));
  addRow.append(b);
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
    renderWidgets(state.widgets);
  });
  invoke("frontend_ready");
  invoke("list_backgrounds").then((list) => {
    options = list || [];
    renderBackgrounds();
  });
}
