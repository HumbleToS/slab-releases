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

const ICON_OPTIONS = [
  "app",
  "discord",
  "steam",
  "spotify",
  "globe",
  "folder",
  "gamepad",
  "terminal",
  "music",
];

/* Editable params per kind. `type` decides the control; every change writes
   one key through widget_set — invalid values are rejected by the backend
   and the field snaps back on the next state push. */
const WIDGET_FIELDS = {
  clock: [
    { key: "format", label: "Format", type: "select", options: ["12h", "24h"] },
    { key: "show_date", label: "Show date", type: "bool" },
  ],
  media: [],
  weather: [
    { key: "label", label: "Location name", type: "text" },
    { key: "lat", label: "Latitude", type: "number" },
    { key: "lon", label: "Longitude", type: "number" },
    {
      key: "unit",
      label: "Unit",
      type: "select",
      options: ["fahrenheit", "celsius"],
    },
  ],
  shortcut: [
    { key: "label", label: "Label", type: "text" },
    { key: "uri", label: "Opens (app, folder, or URL)", type: "text" },
    { key: "icon", label: "Icon", type: "select", options: ICON_OPTIONS },
  ],
  stats: [
    { key: "show_cpu", label: "CPU meter", type: "bool" },
    { key: "show_ram", label: "RAM meter", type: "bool" },
    { key: "show_disk", label: "Drive meter", type: "bool" },
    { key: "disk", label: "Drive letter", type: "text" },
    { key: "disk_label", label: "Drive meter name", type: "text" },
  ],
  text: [
    { key: "text", label: "Text", type: "text" },
    {
      key: "size",
      label: "Size",
      type: "select",
      options: ["small", "medium", "large"],
    },
    { key: "align", label: "Align", type: "select", options: ["left", "center"] },
  ],
};

let expandedWidget = null; // index of the row whose editor is open
let lastWidgets = []; // latest widget state pushed by the backend

const pendingWidgetWrites = new Map();
function writeWidget(index, key, value) {
  const id = `${index}:${key}`;
  clearTimeout(pendingWidgetWrites.get(id));
  pendingWidgetWrites.set(
    id,
    setTimeout(() => invoke("widget_set", { index, key, value }), 300)
  );
}

function buildField(index, widget, field) {
  const wrap = el("label", "widget-field");
  wrap.append(el("span", "widget-field-label dim", field.label));
  let input;
  if (field.type === "select") {
    input = document.createElement("select");
    for (const option of field.options) {
      const node = document.createElement("option");
      node.value = option;
      node.textContent = option;
      input.append(node);
    }
    input.value = widget[field.key];
    input.addEventListener("change", () =>
      writeWidget(index, field.key, input.value)
    );
  } else if (field.type === "bool") {
    input = document.createElement("input");
    input.type = "checkbox";
    input.checked = Boolean(widget[field.key]);
    input.addEventListener("change", () =>
      writeWidget(index, field.key, input.checked)
    );
    wrap.classList.add("widget-field-bool");
  } else {
    input = document.createElement("input");
    input.type = "text";
    input.value = widget[field.key] ?? "";
    input.addEventListener("input", () => {
      if (field.type === "number") {
        const number = parseFloat(input.value);
        if (!Number.isFinite(number)) return; // wait for a complete number
        writeWidget(index, field.key, number);
      } else {
        writeWidget(index, field.key, input.value);
      }
    });
  }
  wrap.append(input);
  return wrap;
}

function buildEditor(index, widget) {
  const editor = el("div", "widget-editor");
  const fields = WIDGET_FIELDS[widget.kind] || [];
  if (!fields.length) {
    editor.append(
      el("div", "widget-summary dim", "Nothing to configure — it just works.")
    );
    return editor;
  }
  for (const field of fields) {
    editor.append(buildField(index, widget, field));
  }
  return editor;
}

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
  // Re-rendering under a focused field would eat the user's typing; the
  // state that matters lands on the next push after they blur.
  const active = document.activeElement;
  if (list.contains(active) && active.matches("input, select")) return;
  list.replaceChildren();
  widgets.forEach((widget, index) => {
    const item = el("div", "widget-item");
    const row = el("div", "widget-row");
    row.append(
      el("span", "widget-caret dim", expandedWidget === index ? "▾" : "▸"),
      el("span", "widget-kind", widget.kind),
      el("span", "widget-summary dim", widgetSummary(widget))
    );
    row.addEventListener("click", () => {
      expandedWidget = expandedWidget === index ? null : index;
      // Always re-render from the freshest backend state — this closure's
      // `widgets` may predate edits made since it was rendered.
      renderWidgets(lastWidgets);
    });
    const actions = el("div", "widget-actions");
    const button = (label, title, disabled, onClick) => {
      const b = el("button", "widget-action", label);
      b.type = "button";
      b.title = title;
      b.disabled = disabled;
      b.addEventListener("click", (e) => {
        e.stopPropagation(); // don't toggle the editor
        onClick();
      });
      actions.append(b);
    };
    button("↑", "Move up", index === 0, () => {
      if (expandedWidget === index) expandedWidget = index - 1;
      invoke("widget_move", { index, up: true });
    });
    button("↓", "Move down", index === widgets.length - 1, () => {
      if (expandedWidget === index) expandedWidget = index + 1;
      invoke("widget_move", { index, up: false });
    });
    button("✕", "Remove", false, () => {
      expandedWidget = null;
      invoke("widget_remove", { index });
    });
    row.append(actions);
    item.append(row);
    if (expandedWidget === index) {
      item.append(buildEditor(index, widget));
    }
    list.append(item);
  });
  if (!widgets.length) {
    list.append(el("div", "widget-summary dim", "No widgets — add one below."));
  }
}

// Refreshes are suppressed while a field has focus; when focus leaves the
// editors, catch up with whatever state arrived in the meantime.
document.getElementById("widget-list").addEventListener("focusout", () => {
  setTimeout(() => {
    const list = document.getElementById("widget-list");
    const active = document.activeElement;
    if (!(list.contains(active) && active.matches("input, select"))) {
      renderWidgets(lastWidgets);
    }
  }, 0);
});

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

document.getElementById("check-updates").addEventListener("click", () => {
  invoke("update_check");
});

/* ---------- update status ---------- */

const UPDATE_LABELS = {
  checking: "checking for updates…",
  current: "up to date",
  error: "update check failed — will retry",
};

function showUpdateStatus(status) {
  const node = document.getElementById("update-status");
  node.textContent =
    status.state === "installing"
      ? `updating to v${status.version}…`
      : UPDATE_LABELS[status.state] || "";
}

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
    lastWidgets = state.widgets;
    renderWidgets(lastWidgets);
    if (state.version) {
      document.getElementById("app-version").textContent = `v${state.version}`;
    }
  });
  tauri.event.listen("update-status", (event) => showUpdateStatus(event.payload));
  invoke("frontend_ready");
  invoke("list_backgrounds").then((list) => {
    options = list || [];
    renderBackgrounds();
  });
}
