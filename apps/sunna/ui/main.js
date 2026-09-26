// Sunna launcher. Everything real happens in Rust commands (src/main.rs);
// this only draws the list and forwards clicks.
const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const $ = (id) => document.getElementById(id);
const REFRESH_MS = 5000;

let machines = [];
let refreshedAt = null;
let refreshing = false;
let connecting = null;
let showOthers = false;

// Machines running a Sunna host; the rest of the tailnet (phones, servers
// that don't host) folds into one line.
const HOSTING = new Set(["ready", "busy", "wrong-token", "update-needed"]);

const STATE_TEXT = {
  ready: "Ready",
  busy: "In a session",
  "wrong-token": "Different session token",
  "update-needed": "Runs a different Sunna version",
  "not-hosting": "Not hosting",
  offline: "Offline",
};

function detail(machine) {
  const parts = [STATE_TEXT[machine.state] ?? machine.state];
  if (machine.os) parts.push(machine.os);
  return parts.join(" · ");
}

function row(machine) {
  const row = $("machine-row").content.firstElementChild.cloneNode(true);
  row.dataset.state = machine.state;
  row.querySelector(".machine-name").textContent = machine.name;
  row.querySelector(".machine-detail").textContent = detail(machine);
  row.title = [machine.dns_name, machine.ip].filter(Boolean).join(" · ");
  const button = row.querySelector(".connect");
  if (machine.state === "ready") {
    button.disabled = connecting !== null;
    button.textContent = connecting === machine.ip ? "Opening…" : "Connect";
    button.addEventListener("click", () => connect(machine));
    row.addEventListener("dblclick", () => connect(machine));
  } else if (machine.state === "wrong-token") {
    // Only offer what fixes it.
    button.className = "text-button";
    button.textContent = "Fix";
    button.addEventListener("click", openSettings);
  } else {
    button.remove();
  }
  return row;
}

function render() {
  const hosting = machines.filter((machine) => HOSTING.has(machine.state));
  const others = machines.filter((machine) => !HOSTING.has(machine.state));
  $("machines").replaceChildren(...hosting.map(row));
  $("others").replaceChildren(...others.map(row));
  const toggle = $("others-toggle");
  toggle.hidden = others.length === 0;
  const noun = others.length === 1 ? "device" : "devices";
  toggle.textContent = showOthers
    ? `Hide ${others.length} other ${noun}`
    : `${others.length} other ${noun} on your tailnet aren't hosting · Show`;
  $("others").hidden = !showOthers || others.length === 0;
  $("empty").hidden = refreshedAt === null || hosting.length > 0;
  updateStatus();
}

function updateStatus() {
  if (refreshedAt === null) return;
  const seconds = Math.round((Date.now() - refreshedAt) / 1000);
  const ready = machines.filter((machine) => machine.state === "ready").length;
  const age = seconds < 2 ? "just now" : `${seconds}s ago`;
  $("status").textContent = `${ready} ready · via Tailscale · updated ${age}`;
}

function showBanner(text, kind = "error") {
  const banner = $("banner");
  banner.textContent = text;
  banner.dataset.kind = kind;
  banner.hidden = !text;
}

async function refresh() {
  if (refreshing) return;
  refreshing = true;
  try {
    machines = await invoke("list_machines");
    refreshedAt = Date.now();
    if ($("banner").dataset.kind === "discovery") showBanner("");
  } catch (error) {
    showBanner(String(error), "discovery");
    $("status").textContent = "Couldn't look on your tailnet";
  } finally {
    refreshing = false;
    render();
  }
}

async function connect(machine) {
  if (connecting !== null) return;
  connecting = machine.ip;
  showBanner("");
  render();
  try {
    await invoke("connect", { ip: machine.ip, name: machine.name });
  } catch (error) {
    showBanner(String(error));
    connecting = null;
    render();
  }
}

// The viewer runs in its own process; the launcher hides while it's open
// and comes back when the session ends.
listen("session-ended", (event) => {
  connecting = null;
  const reason = event.payload?.reason;
  if (reason) showBanner(reason);
  refresh();
});

// Settings
async function openSettings() {
  const settings = await invoke("get_settings");
  $("token").value = settings.token ?? "";
  $("token").type = "password";
  $("toggle-token").textContent = "Show";
  $("windowed").checked = settings.windowed;
  $("stats").checked = settings.stats;
  $("button").checked = settings.menu_button;
  $("saved").hidden = true;
  $("settings").hidden = false;
}

$("open-settings").addEventListener("click", openSettings);
$("close-settings").addEventListener("click", () => {
  $("settings").hidden = true;
  refresh();
});
$("toggle-token").addEventListener("click", () => {
  const hidden = $("token").type === "password";
  $("token").type = hidden ? "text" : "password";
  $("toggle-token").textContent = hidden ? "Hide" : "Show";
});
$("settings-form").addEventListener("submit", async (event) => {
  event.preventDefault();
  try {
    await invoke("set_settings", {
      settings: {
        token: $("token").value.trim(),
        windowed: $("windowed").checked,
        stats: $("stats").checked,
        menu_button: $("button").checked,
      },
    });
    $("saved").hidden = false;
    setTimeout(() => ($("saved").hidden = true), 1500);
  } catch (error) {
    showBanner(String(error));
  }
});
$("refresh").addEventListener("click", refresh);
$("others-toggle").addEventListener("click", () => {
  showOthers = !showOthers;
  render();
});
document.addEventListener("keydown", (event) => {
  if (event.key === "Escape" && !$("settings").hidden) $("close-settings").click();
  if ((event.metaKey || event.ctrlKey) && event.key === ",") {
    event.preventDefault();
    openSettings();
  }
});

refresh();
setInterval(refresh, REFRESH_MS);
setInterval(updateStatus, 1000);
