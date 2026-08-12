// Slab frontend. Renders the widget column from backend state and forwards
// touch intents back as commands. All themable values arrive as CSS custom
// properties via `config-update` — nothing visual is decided here.

import { initThemeControls } from "./theme-controls.js";

const tauri = window.__TAURI__;

function invoke(command, args) {
  if (!tauri) return;
  tauri.core.invoke(command, args).catch((e) => console.error(command, e));
}

// The dashboard runs on a screen with no devtools: every JS failure must
// reach Slab.log via the backend or it is invisible in the field.
window.addEventListener("error", (e) =>
  invoke("frontend_log", { message: `${e.message} (${e.filename}:${e.lineno})` })
);
window.addEventListener("unhandledrejection", (e) =>
  invoke("frontend_log", { message: String(e.reason) })
);

function assetUrl(path) {
  return tauri ? tauri.core.convertFileSrc(path) : path;
}

/* Built-in icon set: simple geometric glyphs drawn with currentcolor.
   A shortcut can also point `icon` at its own image file. */
const STROKE =
  'fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"';
const ICONS = {
  play: '<path d="M8 5v14l11-7z"/>',
  pause: '<path d="M6 5h4v14H6zM14 5h4v14h-4z"/>',
  prev: '<path d="M6 6h2v12H6zM18 6l-8.5 6L18 18z"/>',
  next: '<path d="M16 6h2v12h-2zM6 6l8.5 6L6 18z"/>',
  discord: `<g ${STROKE}><path d="M12 3.5C6.8 3.5 3 7 3 11.5c0 2.6 1.4 4.9 3.6 6.4L5.8 21l3.3-1.6c.9.2 1.9.4 2.9.4 5.2 0 9-3.6 9-8.2S17.2 3.5 12 3.5z"/><circle cx="9" cy="11.5" r="0.5"/><circle cx="15" cy="11.5" r="0.5"/></g>`,
  steam: `<g ${STROKE}><circle cx="12" cy="12" r="9"/><circle cx="15" cy="9" r="2.6"/><circle cx="8.5" cy="15.5" r="1.9"/><path d="M10.2 14.2 13 10.9"/></g>`,
  spotify: `<g ${STROKE}><circle cx="12" cy="12" r="9"/><path d="M7.5 10.5c3-.9 6.3-.6 9 .9M8 13.4c2.5-.7 5.1-.4 7.3.8M8.6 16c2-.5 4-.3 5.7.6"/></g>`,
  globe: `<g ${STROKE}><circle cx="12" cy="12" r="9"/><path d="M3 12h18M12 3a13.5 13.5 0 0 1 0 18M12 3a13.5 13.5 0 0 0 0 18"/></g>`,
  folder: `<g ${STROKE}><path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/></g>`,
  gamepad: `<g ${STROKE}><path d="M6 8h12a4 4 0 0 1 4 4v3a2.5 2.5 0 0 1-4.5 1.5L16 15H8l-1.5 1.5A2.5 2.5 0 0 1 2 15v-3a4 4 0 0 1 4-4z"/><path d="M8 10.5v3M6.5 12h3"/><circle cx="16" cy="11" r="0.5"/><circle cx="18" cy="13" r="0.5"/></g>`,
  terminal: `<g ${STROKE}><rect x="3" y="4.5" width="18" height="15" rx="2"/><path d="m7 9 3 3-3 3M12.5 15H17"/></g>`,
  music: `<g ${STROKE}><path d="M9 17.5V6l11-1.8v11.6"/><circle cx="6.8" cy="17.5" r="2.2"/><circle cx="17.8" cy="15.8" r="2.2"/></g>`,
  app: `<g ${STROKE}><rect x="4" y="4" width="16" height="16" rx="4"/></g>`,
  gear: `<g ${STROKE}><circle cx="12" cy="12" r="3.2"/><path d="M12 2.8v2.6M12 18.6v2.6M2.8 12h2.6M18.6 12h2.6M5.4 5.4l1.9 1.9M16.7 16.7l1.9 1.9M18.6 5.4l-1.9 1.9M7.3 16.7l-1.9 1.9"/></g>`,
};

function svgIcon(name) {
  const body = ICONS[name] || ICONS.app;
  return `<svg viewBox="0 0 24 24" aria-hidden="true">${body}</svg>`;
}

function el(tag, className, text) {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}

/* ---------- live state ---------- */

const clocks = []; // {timeEl, meridiemEl, dateEl, format, showDate}
const weatherWidgets = new Map(); // widget_index -> element refs
const suns = new Map(); // widget_index -> {track, dot, sunrise, sunset, isDay}
const mediaWidgets = []; // element refs, one per media widget
const statsWidgets = new Map(); // widget_index -> {cpu?, ram?, disk?} row refs

/* ---------- rendering ---------- */

function render(state) {
  const root = document.documentElement;
  for (const [name, value] of state.tokens) {
    root.style.setProperty(name, value);
  }
  lastTokens = Object.fromEntries(state.tokens);
  syncSettings();
  applyBackground(state.background);

  clocks.length = 0;
  mediaWidgets.length = 0;
  weatherWidgets.clear();
  suns.clear();
  statsWidgets.clear();
  const container = document.getElementById("widgets");
  container.replaceChildren();

  let shortcutGrid = null;
  state.widgets.forEach((widget, index) => {
    if (widget.kind !== "shortcut") shortcutGrid = null;
    switch (widget.kind) {
      case "clock":
        container.append(buildClock(widget));
        break;
      case "media":
        container.append(buildMedia(widget));
        break;
      case "weather":
        container.append(buildWeather(widget, index));
        break;
      case "stats":
        container.append(buildStats(widget, index));
        break;
      case "text":
        container.append(buildText(widget));
        break;
      case "shortcut":
        // Consecutive shortcuts share one grid.
        if (!shortcutGrid) {
          shortcutGrid = el("div", "shortcut-grid");
          container.append(shortcutGrid);
        }
        shortcutGrid.append(buildShortcut(widget));
        break;
      default:
        // Backend already filters unknown kinds; belt and braces.
        break;
    }
  });
  tick();
}

function applyBackground(background) {
  const video = document.getElementById("bg-video");
  const image = document.getElementById("bg-image");
  const useVideo = background.kind === "video";
  const useImage = background.kind === "image";

  if (useVideo) {
    const url = assetUrl(background.path);
    if (video.dataset.src !== url) {
      video.dataset.src = url;
      video.src = url;
    }
  } else if (video.dataset.src) {
    // Wind the stream down gently — yanking the src mid-request aborts the
    // asset-protocol stream harder than it needs to.
    video.pause();
    video.removeAttribute("src");
    video.load();
    delete video.dataset.src;
  }
  if (useImage) {
    image.src = assetUrl(background.path);
  } else {
    image.removeAttribute("src");
  }
  video.hidden = !useVideo;
  image.hidden = !useImage;
}

// A background file that fails to decode falls back to the gradient.
for (const id of ["bg-video", "bg-image"]) {
  document.getElementById(id).addEventListener("error", (e) => {
    console.error("background failed to load", e);
    e.target.hidden = true;
  });
}

function buildClock(widget) {
  const wrap = el("section", "clock");
  const time = el("div", "clock-time");
  const digits = el("span");
  const meridiem = el("span", "clock-meridiem dim");
  time.append(digits, meridiem);
  wrap.append(time);
  let date = null;
  if (widget.show_date) {
    date = el("div", "clock-date dim");
    wrap.append(date);
  }
  clocks.push({ digits, meridiem, date, format: widget.format });
  return wrap;
}

function buildMedia() {
  const card = el("section", "card media");
  const empty = el("div", "media-empty dim", "No media playing");
  const title = el("div", "media-title");
  const artist = el("div", "media-artist dim");
  const app = el("div", "media-app faint");
  const controls = el("div", "media-controls");

  const buttons = [
    ["prev", "media_prev", "Previous track"],
    ["play", "media_play_pause", "Play or pause"],
    ["next", "media_next", "Next track"],
  ].map(([icon, command, label]) => {
    const button = el("button");
    button.type = "button";
    button.setAttribute("aria-label", label);
    button.innerHTML = svgIcon(icon);
    button.addEventListener("click", () => invoke(command));
    controls.append(button);
    return button;
  });

  card.append(empty, title, artist, app, controls);
  mediaWidgets.push({ card, empty, title, artist, app, controls, playButton: buttons[1] });
  return card;
}

function applyMedia(update) {
  for (const media of mediaWidgets) {
    const empty = update.empty;
    media.empty.hidden = !empty;
    media.title.hidden = empty;
    media.artist.hidden = empty;
    media.app.hidden = empty;
    media.controls.hidden = empty;
    if (empty) continue;
    media.title.textContent = update.title || "Untitled";
    media.artist.textContent = update.artist;
    media.app.textContent = friendlyAppId(update.app_id);
    media.playButton.innerHTML = svgIcon(update.playing ? "pause" : "play");
  }
}

function friendlyAppId(appId) {
  // "Spotify.exe" / "SpotifyAB.SpotifyMusic_zpdnekdrzrea0!Spotify" → "Spotify"
  if (!appId) return "";
  const tail = appId.split("!").pop().split("\\").pop();
  return tail.replace(/\.exe$/i, "").split(".").pop() || appId;
}

function buildWeather(widget, index) {
  const card = el("section", "card weather");
  const top = el("div", "weather-top");
  const temp = el("div", "weather-temp", "—°");
  const condition = el("div", "weather-condition dim");
  top.append(temp, condition);
  const meta = el("div", "weather-meta");
  const label = el("span", "dim", widget.label || "");
  const range = el("span", "dim");
  meta.append(label, range);
  const track = el("div", "sun-track");
  const dot = el("div", "sun-dot");
  dot.hidden = true;
  track.append(dot);
  card.append(top, meta, track);
  weatherWidgets.set(index, { temp, condition, label, range });
  suns.set(index, { dot, sunrise: null, sunset: null, isDay: false });
  return card;
}

function applyWeather(update) {
  const refs = weatherWidgets.get(update.widget_index);
  if (!refs) return;
  const unit = update.unit === "celsius" ? "C" : "F";
  refs.temp.textContent = `${Math.round(update.temperature)}°`;
  refs.condition.textContent = update.condition;
  if (update.label) refs.label.textContent = update.label;
  if (Number.isFinite(update.high) && Number.isFinite(update.low)) {
    refs.range.textContent = `${Math.round(update.high)}° / ${Math.round(update.low)}° ${unit}`;
  }
  const sun = suns.get(update.widget_index);
  if (sun) {
    sun.sunrise = Date.parse(update.sunrise);
    sun.sunset = Date.parse(update.sunset);
    sun.isDay = update.is_day;
    positionSun(sun);
  }
}

function positionSun(sun) {
  const valid =
    Number.isFinite(sun.sunrise) &&
    Number.isFinite(sun.sunset) &&
    sun.sunset > sun.sunrise;
  if (!valid || !sun.isDay) {
    sun.dot.hidden = true;
    return;
  }
  const progress = (Date.now() - sun.sunrise) / (sun.sunset - sun.sunrise);
  sun.dot.hidden = progress < 0 || progress > 1;
  sun.dot.style.left = `${Math.min(100, Math.max(0, progress * 100))}%`;
}

function buildStats(widget, index) {
  const card = el("section", "card stats");
  const refs = {};
  const rows = [
    ["cpu", "CPU", widget.show_cpu],
    ["ram", "RAM", widget.show_ram],
    ["disk", widget.disk_label || "storage", widget.show_disk],
  ];
  for (const [key, name, enabled] of rows) {
    if (!enabled) continue;
    const row = el("div", "stats-row");
    const meter = el("div", "stats-meter");
    const fill = el("div", "stats-fill");
    meter.append(fill);
    const value = el("span", "stats-value dim", "—");
    row.append(el("span", "stats-name dim", name), meter, value);
    card.append(row);
    refs[key] = { fill, value };
  }
  statsWidgets.set(index, refs);
  return card;
}

function applyStats(update) {
  const refs = statsWidgets.get(update.widget_index);
  if (!refs) return;
  const width = (percent) => `${Math.min(100, Math.max(0, percent))}%`;
  if (refs.cpu) {
    refs.cpu.fill.style.width = width(update.cpu_percent);
    refs.cpu.value.textContent = `${Math.round(update.cpu_percent)}%`;
  }
  if (refs.ram) {
    refs.ram.fill.style.width = width(update.ram_percent);
    refs.ram.value.textContent = update.ram_text;
  }
  if (refs.disk && update.disk_percent !== null) {
    refs.disk.fill.style.width = width(update.disk_percent);
    refs.disk.value.textContent = update.disk_text;
  }
}

function buildText(widget) {
  return el(
    "section",
    `text-widget text-${widget.size} align-${widget.align}`,
    widget.text
  );
}

function buildShortcut(widget) {
  const tile = el("button", "tile");
  tile.type = "button";
  const icon = widget.icon || "app";
  if (ICONS[icon]) {
    tile.innerHTML = svgIcon(icon);
  } else if (/\.[a-z0-9]+$/i.test(icon)) {
    const img = document.createElement("img");
    img.src = assetUrl(icon);
    img.alt = "";
    img.addEventListener("error", () => {
      img.replaceWith(iconNode("app"));
    });
    tile.append(img);
  } else {
    tile.innerHTML = svgIcon("app");
  }
  tile.append(el("span", "tile-label", widget.label || widget.uri));
  tile.addEventListener("click", () => invoke("open_shortcut", { uri: widget.uri }));
  return tile;
}

function iconNode(name) {
  const span = el("span");
  span.innerHTML = svgIcon(name);
  return span.firstChild;
}

/* ---------- clock + sun ticker ---------- */

function tick() {
  const now = new Date();
  for (const clock of clocks) {
    let hours = now.getHours();
    let meridiem = "";
    if (clock.format !== "24h") {
      meridiem = hours < 12 ? "AM" : "PM";
      hours = hours % 12 || 12;
    }
    const minutes = String(now.getMinutes()).padStart(2, "0");
    const display = clock.format === "24h" ? String(hours).padStart(2, "0") : String(hours);
    clock.digits.textContent = `${display}:${minutes}`;
    clock.meridiem.textContent = meridiem;
    if (clock.date) {
      clock.date.textContent = now.toLocaleDateString(undefined, {
        weekday: "long",
        month: "long",
        day: "numeric",
      });
    }
  }
  for (const sun of suns.values()) positionSun(sun);
}

setInterval(tick, 1000);

/* ---------- quick theme panel (gear, touch-first) ---------- */

let lastTokens = {};
let themeSync = null;

function buildSettings() {
  const card = document.getElementById("settings-card");
  const toggle = document.getElementById("settings-toggle");
  toggle.innerHTML = svgIcon("gear");
  toggle.addEventListener("click", () => {
    card.hidden = !card.hidden;
    if (!card.hidden) syncSettings();
  });
  themeSync = initThemeControls(card, invoke);
}

function syncSettings() {
  if (themeSync) themeSync(lastTokens);
}

buildSettings();

/* ---------- wiring ---------- */

document.addEventListener("contextmenu", (e) => e.preventDefault());

if (tauri) {
  tauri.event.listen("config-update", (event) => render(event.payload));
  tauri.event.listen("weather-update", (event) => applyWeather(event.payload));
  tauri.event.listen("media-update", (event) => applyMedia(event.payload));
  tauri.event.listen("stats-update", (event) => applyStats(event.payload));
  invoke("frontend_ready");
}
