// Sunna launcher. Everything real happens in Rust commands (src/main.rs);
// this draws the views and forwards what the user does.
const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const $ = (id) => document.getElementById(id);
const REFRESH_MS = 5000;

// Machines running a Sunna host; the rest of the tailnet (phones, servers
// that don't host) is listed quietly below.
const HOSTING = new Set(["ready", "busy", "wrong-token", "update-needed"]);

const PILL = {
  ready: "Ready",
  busy: "In use",
  "wrong-token": "Token mismatch",
  "update-needed": "Update needed",
};
const OTHER = {
  "not-hosting": "Not sharing",
  offline: "Offline",
};

const state = {
  scan: null, // { this_device, tailnet, running, machines }
  scanning: false,
  error: null,
  connecting: null, // ip
  query: "",
};

// ───────── Views ─────────

function show(view) {
  for (const item of document.querySelectorAll(".nav-item")) {
    if (item.dataset.view === view) item.setAttribute("aria-current", "page");
    else item.removeAttribute("aria-current");
  }
  $("view-computers").hidden = view !== "computers";
  $("view-settings").hidden = view !== "settings";
  if (view === "settings") loadSettings();
}

for (const item of document.querySelectorAll(".nav-item")) {
  item.addEventListener("click", () => show(item.dataset.view));
}

// ───────── Computers ─────────

function osKind(os) {
  const name = (os || "").toLowerCase();
  if (name.includes("mac")) return "macos";
  if (name.includes("linux")) return "linux";
  if (name.includes("windows")) return "windows";
  return "other";
}

function card(machine) {
  const node = $("card-template").content.firstElementChild.cloneNode(true);
  const kind = osKind(machine.os);
  node.dataset.state = machine.state;
  node.dataset.os = kind;
  node.dataset.ip = machine.ip;
  node.querySelector(".device use").setAttribute("href", kind === "macos" ? "#i-laptop" : "#i-desktop");
  node.querySelector(".card-name").textContent = machine.name;
  node.querySelector(".card-os").textContent = machine.os || "";
  const pill = node.querySelector(".pill");
  pill.textContent = PILL[machine.state] ?? machine.state;
  pill.classList.add(machine.state);
  node.classList.toggle("connecting", state.connecting === machine.ip);

  if (machine.state === "ready") {
    node.setAttribute("aria-label", `Connect to ${machine.name}`);
    node.addEventListener("click", () => connect(machine));
  } else if (machine.state === "wrong-token") {
    node.setAttribute("aria-label", `${machine.name}: uses a different session token. Open settings.`);
    node.addEventListener("click", () => {
      show("settings");
      $("token").focus();
      toast("That computer uses a different session token. Paste the same token here as on that computer.", "info");
    });
  } else if (machine.state === "busy") {
    node.addEventListener("click", () => toast(`Someone is already connected to ${machine.name}.`, "info"));
  } else {
    node.addEventListener("click", () =>
      toast(`${machine.name} runs a different version of Sunna. Update both computers.`, "info"),
    );
  }
  return node;
}

function skeletons(count) {
  return Array.from({ length: count }, () => {
    const node = document.createElement("div");
    node.className = "card skeleton";
    node.innerHTML =
      '<span class="card-art"></span><span class="card-body"><span class="skeleton-line long"></span><span class="skeleton-line short"></span></span>';
    return node;
  });
}

function otherRow(machine) {
  const row = document.createElement("li");
  row.className = "other";
  row.title = [machine.dns_name, machine.ip].filter(Boolean).join(" · ");
  const kind = osKind(machine.os);
  row.innerHTML = `<svg class="icon"><use href="${kind === "macos" ? "#i-laptop" : "#i-computers"}"/></svg><span class="other-text"><span class="other-name"></span><span class="other-state"></span></span>`;
  row.querySelector(".other-name").textContent = machine.name;
  row.querySelector(".other-state").textContent = [OTHER[machine.state] ?? machine.state, machine.os]
    .filter(Boolean)
    .join(" · ");
  return row;
}

function matches(machine) {
  const query = state.query.trim().toLowerCase();
  return !query || `${machine.name} ${machine.os} ${machine.ip}`.toLowerCase().includes(query);
}

function render() {
  const scan = state.scan;
  const grid = $("grid");
  if (!scan) {
    grid.replaceChildren(...(state.error ? [] : skeletons(3)));
    $("empty").hidden = true;
    $("others").hidden = true;
    $("subtitle").textContent = state.error ? "Couldn't look on your tailnet" : "Looking on your tailnet…";
    renderTailnet();
    return;
  }
  const machines = scan.machines.filter(matches);
  const hosting = machines.filter((machine) => HOSTING.has(machine.state));
  const others = machines.filter((machine) => !HOSTING.has(machine.state));
  const focused = document.activeElement?.dataset?.ip;
  grid.replaceChildren(...hosting.map(card));
  if (focused) grid.querySelector(`[data-ip="${CSS.escape(focused)}"]`)?.focus();

  const ready = scan.machines.filter((machine) => machine.state === "ready").length;
  const sharing = scan.machines.filter((machine) => HOSTING.has(machine.state)).length;
  $("subtitle").textContent =
    sharing === 0
      ? "Nothing is sharing yet"
      : `${ready} ready${sharing > ready ? ` · ${sharing - ready} unavailable` : ""}`;
  $("nav-count").hidden = ready === 0;
  $("nav-count").textContent = ready;
  $("empty").hidden = sharing > 0 || state.query !== "";
  $("search-box").hidden = scan.machines.length < 5 && state.query === "";

  $("others").hidden = others.length === 0;
  $("others-summary").textContent =
    `${others.length} other ${others.length === 1 ? "device" : "devices"} on your tailnet`;
  $("others-list").replaceChildren(...others.map(otherRow));
  renderTailnet();
}

function renderTailnet() {
  const dot = $("tailnet-dot");
  if (state.error) {
    dot.className = "status-dot off";
    $("tailnet-name").textContent = "Tailscale";
    $("tailnet-detail").textContent = "Not connected";
    return;
  }
  if (!state.scan) return;
  dot.className = "status-dot on";
  $("tailnet-name").textContent = "Tailscale connected";
  $("tailnet-detail").textContent = state.scan.this_device ? `This computer: ${state.scan.this_device}` : "";
}

async function refresh() {
  if (state.scanning) return;
  state.scanning = true;
  $("refresh").classList.add("spinning");
  try {
    state.scan = await invoke("scan");
    state.error = null;
  } catch (error) {
    state.error = String(error);
    if (!state.scan) toast(state.error);
  } finally {
    state.scanning = false;
    // Let the spin finish a turn; a flicker reads as a glitch.
    setTimeout(() => $("refresh").classList.remove("spinning"), 400);
    render();
  }
}

async function connect(machine) {
  if (state.connecting) return;
  state.connecting = machine.ip;
  hideToast();
  render();
  try {
    await invoke("connect", { ip: machine.ip, name: machine.name });
  } catch (error) {
    state.connecting = null;
    toast(String(error));
    render();
  }
}

// The session runs in its own window; the app hides until it ends.
listen("session-ended", (event) => {
  state.connecting = null;
  const reason = event.payload?.reason;
  if (reason) toast(reason);
  refresh();
});

$("refresh").addEventListener("click", refresh);
$("search").addEventListener("input", (event) => {
  state.query = event.target.value;
  render();
});
$("copy-command").addEventListener("click", (event) => copy("scripts/dogfood.sh host", event.currentTarget));

// Arrow keys move between cards; Enter/Space activate (they're buttons).
$("grid").addEventListener("keydown", (event) => {
  const cards = [...$("grid").querySelectorAll(".card:not(.skeleton)")];
  const index = cards.indexOf(document.activeElement);
  if (index < 0) return;
  const columns = getComputedStyle($("grid")).gridTemplateColumns.split(" ").length;
  const step = { ArrowRight: 1, ArrowLeft: -1, ArrowDown: columns, ArrowUp: -columns }[event.key];
  if (!step) return;
  event.preventDefault();
  cards[Math.max(0, Math.min(cards.length - 1, index + step))].focus();
});

// ───────── Settings ─────────

let settings = null;
let saveTimer = null;

async function loadSettings() {
  settings = await invoke("get_settings");
  $("token").value = settings.token ?? "";
  $("stats").checked = settings.stats;
  $("menu-button").checked = settings.menu_button;
  for (const button of $("display-mode").querySelectorAll("button")) {
    const window = button.dataset.value === "window";
    button.setAttribute("aria-checked", String(window === settings.windowed));
  }
}

function save(delay = 0) {
  clearTimeout(saveTimer);
  saveTimer = setTimeout(async () => {
    try {
      await invoke("set_settings", { settings });
    } catch (error) {
      toast(String(error));
    }
  }, delay);
}

$("token").addEventListener("input", (event) => {
  settings.token = event.target.value.trim();
  save(500);
});
$("stats").addEventListener("change", (event) => {
  settings.stats = event.target.checked;
  save();
});
$("menu-button").addEventListener("change", (event) => {
  settings.menu_button = event.target.checked;
  save();
});
$("display-mode").addEventListener("click", (event) => {
  const button = event.target.closest("button");
  if (!button) return;
  settings.windowed = button.dataset.value === "window";
  for (const other of $("display-mode").querySelectorAll("button")) {
    other.setAttribute("aria-checked", String(other === button));
  }
  save();
});
$("reveal-token").addEventListener("click", () => {
  const hidden = $("token").type === "password";
  $("token").type = hidden ? "text" : "password";
  $("reveal-token").title = hidden ? "Hide" : "Show";
});
$("copy-token").addEventListener("click", (event) => copy($("token").value, event.currentTarget));

// ───────── Bits ─────────

let toastTimer = null;
function toast(text, kind = "error") {
  $("toast-text").textContent = text;
  $("toast").className = `toast ${kind}`;
  $("toast").hidden = false;
  clearTimeout(toastTimer);
  toastTimer = setTimeout(hideToast, kind === "error" ? 9000 : 5000);
}
function hideToast() {
  $("toast").hidden = true;
}
$("toast").addEventListener("click", hideToast);

async function copy(text, button) {
  try {
    await navigator.clipboard.writeText(text);
    const use = button.querySelector("use");
    const before = use.getAttribute("href");
    use.setAttribute("href", "#i-check");
    button.classList.add("done");
    setTimeout(() => {
      use.setAttribute("href", before);
      button.classList.remove("done");
    }, 1200);
  } catch {
    toast("Couldn't copy. Select the text and press ⌘C.", "info");
  }
}

document.addEventListener("keydown", (event) => {
  const command = event.metaKey || event.ctrlKey;
  if (command && event.key === "r") {
    event.preventDefault();
    refresh();
  } else if (command && (event.key === "," || event.key === "2")) {
    event.preventDefault();
    show("settings");
  } else if (command && event.key === "1") {
    event.preventDefault();
    show("computers");
  } else if (command && event.key === "f" && !$("search-box").hidden) {
    event.preventDefault();
    $("search").focus();
  } else if (event.key === "Escape") {
    hideToast();
    if (document.activeElement === $("search")) $("search").blur();
  }
});

(async function start() {
  try {
    const info = await invoke("app_info");
    if (info.vibrancy) document.documentElement.classList.add("vibrancy");
    $("about").textContent = `Sunna ${info.version} · protocol ${info.protocol}`;
  } catch {}
  render();
  refresh();
  setInterval(refresh, REFRESH_MS);
})();
