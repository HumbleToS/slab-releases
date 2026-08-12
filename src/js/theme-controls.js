// Shared theme controls: accent presets + sliders, used by the dashboard's
// quick panel and the customization window. Controls never mutate app state
// directly — each change writes one [theme] key via set_theme and the config
// watcher round-trips it like a hand edit.

const ACCENT_PRESETS = ["#e4553b", "#3ba7e4", "#7bd88f", "#e4b13b", "#c678dd", "#f2f2f4"];

const SLIDERS = [
  ["Panel opacity", "panel_opacity", "--panel-opacity", 0, 1, 0.01, true],
  ["Background shade", "shade", "--shade", 0, 1, 0.01, false],
  ["Text size", "text_scale", "--text-scale", 0.6, 1.6, 0.05, true],
];

export function initThemeControls(container, invoke) {
  const pending = new Map();
  const write = (key, value) => {
    clearTimeout(pending.get(key));
    pending.set(key, setTimeout(() => invoke("set_theme", { key, value }), 250));
  };
  const make = (tag, className, text) => {
    const node = document.createElement(tag);
    if (className) node.className = className;
    if (text !== undefined) node.textContent = text;
    return node;
  };

  const accentRow = make("div", "settings-row");
  accentRow.append(make("div", "settings-label dim", "Accent"));
  const grid = make("div", "swatch-grid");
  for (const color of ACCENT_PRESETS) {
    const swatch = make("button", "swatch");
    swatch.type = "button";
    swatch.dataset.color = color;
    swatch.style.background = color;
    swatch.setAttribute("aria-label", `Accent ${color}`);
    swatch.addEventListener("click", () => {
      document.documentElement.style.setProperty("--accent", color);
      markActive(color);
      write("accent", color);
    });
    grid.append(swatch);
  }
  accentRow.append(grid);
  container.append(accentRow);

  const inputs = [];
  for (const [label, key, token, min, max, step, optimistic] of SLIDERS) {
    const row = make("div", "settings-row");
    row.append(make("div", "settings-label dim", label));
    const input = document.createElement("input");
    input.type = "range";
    input.min = min;
    input.max = max;
    input.step = step;
    input.dataset.token = token;
    input.addEventListener("input", () => {
      // Optimistic for direct pass-through tokens; derived ones (shade's
      // overlay color) wait for the backend round-trip.
      if (optimistic) {
        document.documentElement.style.setProperty(token, input.value);
      }
      write(key, Number(input.value));
    });
    row.append(input);
    container.append(row);
    inputs.push(input);
  }

  function markActive(color) {
    for (const swatch of grid.querySelectorAll(".swatch")) {
      swatch.classList.toggle(
        "active",
        swatch.dataset.color.toLowerCase() === (color || "").toLowerCase()
      );
    }
  }

  return function sync(tokens) {
    for (const input of inputs) {
      if (input === document.activeElement) continue; // mid-drag
      const value = tokens[input.dataset.token];
      if (value !== undefined) input.value = parseFloat(value);
    }
    markActive(tokens["--accent"]);
  };
}
