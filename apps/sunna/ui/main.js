// Sunna launcher. The Rust side (src/main.rs) does the real work: saved
// machines, probing, sessions. This draws them and forwards what you do.

import { animate, flip, snapshot, springs, tilt, wait } from "./motion.js";

// Outside the app (a plain browser), stand in for the Rust side.
if (!window.__TAURI__) await import("../dev/mock.js");
const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const $ = (id) => document.getElementById(id);
const POLL_MS = 4000;
/** States in which a machine's screen is lit: something answered. */
const LIT = new Set(["ready", "busy", "wrong-key", "update-needed"]);

const state = {
  info: { computer: "" },
  machines: [],
  /** id → the last check (state, os, rtt...). */
  checks: new Map(),
  loaded: false,
  polled: false,
  /** { id, phase } while connecting or in a session. */
  session: null,
  /** New computers start with this computer's key. */
  defaultKey: "",
};

const tiles = new Map();
let addTile = null;

// ───────────── Machines on screen ─────────────

function osKind(os = "") {
  const name = os.toLowerCase();
  if (name.includes("mac")) return "macos";
  if (name.includes("windows")) return "windows";
  for (const distro of ["arch", "ubuntu", "fedora", "debian", "mint", "manjaro", "nixos"]) {
    if (name.includes(distro)) return distro;
  }
  if (name.startsWith("pop")) return "pop";
  if (name.includes("linux")) return "linux";
  return "unknown";
}

/** Our own marks: ⌘ for macOS, the penguin for Linux, a plain window for
 *  Windows. Never the vendors' logos (their trademark rules don't allow it). */
function markOf(kind) {
  if (kind === "macos") return "os-command";
  if (kind === "windows") return "os-window";
  if (kind === "unknown") return "os-screen";
  return "os-tux";
}

const DEVICES = { laptop: "Laptop", desktop: "Desktop", vm: "VM", server: "Server" };

/** "Arch Linux · Desktop", "macOS 26.0 · MacBook Air"; the address when
 *  we don't know what it is yet. */
function systemText(machine, check) {
  const os = check?.os || machine.os;
  if (!os) return machine.address ?? "";
  const model = check?.model || machine.model;
  return [os, model || DEVICES[check?.device || machine.device]].filter(Boolean).join(" · ");
}

/** What a Linux boot screen says under the penguin: the distro, no version. */
function bootName(kind, os = "") {
  if (["macos", "windows", "unknown"].includes(kind)) return "";
  return os.replace(/\s*\(.*\)$/, "").replace(/\s+[\d.]+.*$/, "");
}

function paintMark(badge, kind) {
  badge.classList.toggle("tux", markOf(kind) === "os-tux");
  badge.querySelector("use").setAttribute("href", `#${markOf(kind)}`);
}

function hostOf(address = "") {
  const text = address.trim().replace(/^sunna:\/\//, "").split(/[?/#]/)[0];
  if (text.startsWith("[")) return text.slice(1, text.indexOf("]"));
  return (text.match(/:/g) || []).length === 1 ? text.split(":")[0] : text;
}

function ms(value) {
  return value < 1 ? "<1 ms" : `${Math.round(value)} ms`;
}

function ago(seconds) {
  const diff = Date.now() / 1000 - seconds;
  if (diff < 90) return "just now";
  if (diff < 3600) return `${Math.round(diff / 60)} min ago`;
  if (diff < 86400) return `${Math.round(diff / 3600)} h ago`;
  const days = Math.round(diff / 86400);
  return days === 1 ? "yesterday" : `${days} days ago`;
}

function describe(machine, check) {
  switch (check?.state ?? "checking") {
    case "idle":
      return "Enter its address";
    case "checking":
      return "Looking…";
    case "ready":
      return check.rtt_ms != null ? `Ready · ${ms(check.rtt_ms)}` : "Ready";
    case "busy":
      return check.viewer ? `In use · ${check.viewer}` : "In use";
    case "wrong-key":
      return "Key doesn't match";
    case "update-needed":
      return "Needs an update";
    case "not-found":
      return "Address not found";
    case "invalid":
      return "Address isn't valid";
    default:
      return machine.last_seen ? `Offline · seen ${ago(machine.last_seen)}` : "Not responding";
  }
}

/** Size the screen to the machine's display shape inside a 16:10 area. */
function shape(tile, width, height) {
  let ratio = width > 0 && height > 0 ? width / height : 1.6;
  ratio = Math.min(2.4, Math.max(1.25, ratio));
  const [sw, sh] = ratio >= 1.6 ? [100, (1.6 / ratio) * 100] : [(ratio / 1.6) * 100, 100];
  tile.style.setProperty("--sw", `${sw.toFixed(2)}%`);
  tile.style.setProperty("--sh", `${sh.toFixed(2)}%`);
}

/** The screen comes on; machines that let us in boot first. */
function wake(tile, delay = 0, boot = false) {
  tile.classList.remove("sleeping", "waking", "booting");
  tile.style.setProperty("--wake-delay", `${delay}ms`);
  void tile.offsetWidth;
  tile.classList.add("waking");
  tile.classList.toggle("booting", boot);
  clearTimeout(tile.wakeTimer);
  tile.wakeTimer = setTimeout(() => tile.classList.remove("waking", "booting"), 2300 + delay);
}

function sleep(tile) {
  tile.classList.remove("waking", "booting");
  tile.classList.add("sleeping");
  clearTimeout(tile.wakeTimer);
  tile.wakeTimer = setTimeout(() => tile.classList.remove("sleeping"), 900);
}

/** Draw a machine and its state onto a tile (grid or preview). */
function paintTile(tile, machine, check, wakeDelay = 0) {
  const now = check?.state ?? "checking";
  const lit = LIT.has(now);
  const wasLit = tile.hasAttribute("data-lit");
  const kind = osKind(check?.os || machine.os);
  tile.dataset.state = now;
  tile.dataset.os = kind;
  tile.dataset.device = check?.device || machine.device || "";
  tile.toggleAttribute("data-lit", lit);
  if (lit && !wasLit) wake(tile, wakeDelay, now === "ready" || now === "busy");
  if (!lit && wasLit) sleep(tile);
  shape(tile, check?.width || machine.width, check?.height || machine.height);
  const status = describe(machine, check);
  tile.querySelector(".name").textContent = machine.name;
  tile.querySelector(".status-text").textContent = status;
  tile.querySelector(".system-text").textContent = systemText(machine, check);
  paintMark(tile.querySelector(".os-badge"), kind);
  tile.querySelector(".boot-mark use").setAttribute("href", `#${markOf(kind)}`);
  tile.querySelector(".boot-name").textContent = bootName(kind, check?.os || machine.os);
  tile.querySelector(".state-glyph use").setAttribute("href", now === "update-needed" ? "#i-up" : "#i-lock");
  const hit = tile.querySelector(".tile-hit");
  hit.setAttribute("aria-label", `${machine.name}. ${status}`);
  hit.title = [check?.os || machine.os, machine.model, machine.address].filter(Boolean).join(" · ");
}

function createTile(id) {
  const tile = $("tile-template").content.firstElementChild.cloneNode(true);
  tile.dataset.id = id;
  const hit = tile.querySelector(".tile-hit");
  tilt(tile, hit);
  hit.addEventListener("click", () => activate(id));
  hit.addEventListener("contextmenu", (event) => {
    event.preventDefault();
    openMenu(id, { x: event.clientX, y: event.clientY });
  });
  const more = tile.querySelector(".tile-more");
  more.addEventListener("click", (event) => {
    event.stopPropagation();
    openMenu(id, { anchor: more });
  });
  return tile;
}

function createPreviewTile() {
  const tile = $("tile-template").content.firstElementChild.cloneNode(true);
  tile.querySelector(".tile-more").remove();
  tile.removeAttribute("role");
  tile.querySelector(".tile-hit").tabIndex = -1;
  return tile;
}

function createAddTile() {
  const tile = $("add-tile-template").content.firstElementChild.cloneNode(true);
  const hit = tile.querySelector(".tile-hit");
  hit.addEventListener("click", () => openMachineSheet({ opener: hit }));
  return tile;
}

function renderGrid({ stagger = false } = {}) {
  const grid = $("grid");
  const ids = new Set(state.machines.map((machine) => machine.id));
  for (const [id, tile] of tiles) {
    if (!ids.has(id)) {
      tile.remove();
      tiles.delete(id);
    }
  }
  state.machines.forEach((machine, index) => {
    let tile = tiles.get(machine.id);
    if (!tile) {
      tile = createTile(machine.id);
      tile.style.setProperty("--i", index);
      tiles.set(machine.id, tile);
    }
    paintTile(tile, machine, state.checks.get(machine.id), stagger ? 120 + index * 110 : 0);
    const at = grid.children[index];
    if (at !== tile) grid.insertBefore(tile, at ?? null);
  });
  addTile ??= createAddTile();
  addTile.style.setProperty("--i", state.machines.length);
  if (grid.lastElementChild !== addTile) grid.appendChild(addTile);
  const count = state.machines.length + 1;
  grid.dataset.count = count <= 3 ? "few" : count <= 6 ? "some" : "many";
  const empty = state.loaded && state.machines.length === 0;
  grid.hidden = empty;
  $("welcome").hidden = !empty;
  renderHeader();
}

function summary() {
  const machines = state.machines;
  if (!state.loaded) return " ";
  if (!machines.length) return "None added yet";
  if (!state.polled) return "Looking for your computers…";
  const ready = machines.filter((machine) => state.checks.get(machine.id)?.state === "ready");
  if (machines.length === 1) {
    const [machine] = machines;
    return ready.length ? `${machine.name} is ready` : `${machine.name} isn't available`;
  }
  if (ready.length === machines.length) return `All ${machines.length} ready`;
  if (!ready.length) return `None of ${machines.length} available`;
  return `${ready.length} of ${machines.length} ready`;
}

function renderHeader() {
  $("summary").textContent = summary();
}

const byId = (id) => state.machines.find((machine) => machine.id === id);

let polling = false;
async function poll() {
  if (polling || state.session || document.hidden || !state.machines.length) return;
  polling = true;
  try {
    const checks = await invoke("machine_statuses");
    const first = !state.polled;
    for (const [id, check] of Object.entries(checks)) state.checks.set(id, check);
    state.polled = true;
    renderGrid({ stagger: first });
  } catch (error) {
    console.warn("status check failed", error);
  } finally {
    polling = false;
  }
}

/** A click on a machine: connect, or say why not. */
function activate(id) {
  const machine = byId(id);
  if (!machine) return;
  const check = state.checks.get(id);
  switch (check?.state) {
    case "ready":
      return connect(machine);
    case "wrong-key":
      return openMachineSheet({ machine, opener: tiles.get(id), focus: "key" });
    case "busy":
      return toast(`${check.viewer || "Someone"} is connected to ${machine.name} right now.`, "info");
    case "update-needed":
      return toast(check.detail || `${machine.name} runs a different version of Sunna.`, "info");
    case undefined:
    case "checking":
      return toast(`Still looking for ${machine.name}…`, "info");
    default:
      return toast(check.detail || `${machine.name} isn't responding.`, "info", {
        label: "Edit",
        run: () => openMachineSheet({ machine, opener: tiles.get(id) }),
      });
  }
}

// ───────────── Connecting ─────────────

const PHASES = {
  reaching: (name) => `Reaching ${name}…`,
  video: () => "Starting video…",
  open: () => "Connected",
  ended: () => "Disconnected",
};

function setPhase(phase, name) {
  const line = $("launch-phase");
  line.textContent = PHASES[phase]?.(name) ?? "";
  line.classList.remove("swap");
  void line.offsetWidth;
  line.classList.add("swap");
  $("launch-progress").dataset.phase = phase;
  // Only a connection still on its way can be cancelled.
  $("launch-cancel").hidden = phase === "open" || phase === "ended";
}

const px = (rect, radius) => ({
  left: `${rect.left}px`,
  top: `${rect.top}px`,
  width: `${rect.width}px`,
  height: `${rect.height}px`,
  borderRadius: `${radius}px`,
});

/** Where an element in the app sits once the app is at rest, undoing the
 *  app's current scale (receded behind a sheet, or zoomed while launching). */
function restingRect(element) {
  const rect = element.getBoundingClientRect();
  const transform = getComputedStyle($("app")).transform;
  const scale = transform && transform !== "none" ? new DOMMatrixReadOnly(transform).a : 1;
  if (Math.abs(scale - 1) < 0.0005) return rect;
  // .app scales around 50% 40% (style.css).
  const [ox, oy] = [innerWidth * 0.5, innerHeight * 0.4];
  return {
    left: ox + (rect.left - ox) / scale,
    top: oy + (rect.top - oy) / scale,
    width: rect.width / scale,
    height: rect.height / scale,
  };
}

const launch = {
  timer: 0,

  async open(machine) {
    const tile = tiles.get(machine.id);
    const overlay = $("launch");
    const box = $("launch-screen");
    const from = tile.querySelector(".screen").getBoundingClientRect();
    overlay.dataset.os = tile.dataset.os;
    overlay.querySelector(".launch-mark use").setAttribute("href", `#${markOf(tile.dataset.os)}`);
    // Start where the tile's wallpaper is in its drift, so the hand-off is seamless.
    box.querySelector(".wall").style.transform = getComputedStyle(tile.querySelector(".wall")).transform;
    $("launch-name").textContent = machine.name;
    setPhase("reaching", machine.name);
    overlay.classList.remove("expanded");
    overlay.hidden = false;
    tile.classList.add("launch-source");
    $("app").classList.add("launching");
    const full = { left: 0, top: 0, width: innerWidth, height: innerHeight };
    Object.assign(box.style, px(full, 0));
    this.timer = setTimeout(() => overlay.classList.add("expanded"), 160);
    await animate(box, [px(from, 12), px(full, 0)], springs.zoom);
  },

  async close(id) {
    const overlay = $("launch");
    const box = $("launch-screen");
    const tile = tiles.get(id);
    clearTimeout(this.timer);
    overlay.classList.remove("expanded");
    if (tile?.isConnected) {
      const to = restingRect(tile.querySelector(".screen"));
      $("app").classList.remove("launching");
      tile.classList.remove("launch-source");
      const full = box.getBoundingClientRect();
      const move = animate(box, [px(full, 0), px(to, 12)], springs.zoom, { fill: "forwards" });
      // Hand back to the tile's own screen as it lands.
      const fade = box.animate([{ opacity: 1 }, { opacity: 0 }], {
        duration: springs.zoom.duration * 0.35,
        delay: springs.zoom.duration * 0.5,
        fill: "forwards",
      }).finished.catch(() => {});
      await Promise.all([move, fade]);
    } else {
      $("app").classList.remove("launching");
    }
    overlay.hidden = true;
    for (const animation of box.getAnimations()) animation.cancel();
    tile?.classList.remove("launch-source");
  },
};

async function connect(machine) {
  if (state.session) return;
  hideMenu();
  hideToast();
  state.session = { id: machine.id, phase: "reaching" };
  const opening = launch.open(machine);
  try {
    await invoke("connect", { id: machine.id });
  } catch (error) {
    await opening;
    await wait(250);
    await launch.close(machine.id);
    state.session = null;
    toast(String(error));
  }
}

listen("session-progress", ({ payload }) => {
  if (state.session?.id !== payload.id) return;
  state.session.phase = payload.phase;
  setPhase(payload.phase, byId(payload.id)?.name ?? "");
});

listen("session-ended", async ({ payload }) => {
  if (state.session?.id !== payload.id) return;
  if (payload.opened) {
    setPhase("ended", "");
    await wait(500);
  } else {
    await wait(150);
  }
  await launch.close(payload.id);
  state.session = null;
  if (payload.reason) toast(payload.reason);
  // The host frees its viewer slot as the session ends: look again now.
  setTimeout(poll, 300);
});

$("launch-cancel").addEventListener("click", () => invoke("cancel_connect"));

// ───────────── Sheets ─────────────

const sheets = [];

function showScrim() {
  const scrim = $("scrim");
  scrim.hidden = false;
  requestAnimationFrame(() => scrim.classList.add("shown"));
  $("app").classList.add("receded");
}

function hideScrim() {
  const scrim = $("scrim");
  scrim.classList.remove("shown");
  $("app").classList.remove("receded");
  setTimeout(() => {
    if (!sheets.length) scrim.hidden = true;
  }, 320);
}

/** Open a sheet from a template, growing out of `opener`. */
function openSheet(templateId, opener) {
  const sheet = $(templateId).content.firstElementChild.cloneNode(true);
  $("sheets").appendChild(sheet);
  if (!sheets.length) showScrim();
  sheets.push({ sheet, restore: document.activeElement });
  const rect = sheet.getBoundingClientRect();
  const from = opener?.getBoundingClientRect?.();
  sheet.style.transformOrigin = from
    ? `${from.left + from.width / 2 - rect.left}px ${from.top + from.height / 2 - rect.top}px`
    : "50% 50%";
  animate(sheet, [{ transform: "scale(0.86)", opacity: 0 }, { transform: "none", opacity: 1 }], springs.bouncy);
  return sheet;
}

async function closeSheet(sheet) {
  const index = sheets.findIndex((entry) => entry.sheet === sheet);
  if (index < 0) return;
  const [entry] = sheets.splice(index, 1);
  if (!sheets.length) hideScrim();
  sheet.style.pointerEvents = "none";
  await sheet
    .animate([{ transform: "none", opacity: 1 }, { transform: "scale(0.94)", opacity: 0 }], {
      duration: 170,
      easing: "cubic-bezier(0.4, 0, 1, 1)",
      fill: "forwards",
    })
    .finished.catch(() => {});
  sheet.remove();
  if (entry.restore?.isConnected) entry.restore.focus({ preventScroll: true });
}

const topSheet = () => sheets.at(-1)?.sheet;

$("scrim").addEventListener("click", () => {
  const sheet = topSheet();
  if (sheet) closeSheet(sheet);
});

function focusables(root) {
  return [...root.querySelectorAll("button, input, [tabindex]")].filter(
    (element) => !element.disabled && element.tabIndex >= 0 && element.offsetParent !== null,
  );
}

async function switchPage(sheet, from, to, forward) {
  const before = sheet.getBoundingClientRect().height;
  from.hidden = true;
  to.hidden = false;
  const after = sheet.getBoundingClientRect().height;
  animate(sheet, [{ height: `${before}px` }, { height: `${after}px` }], springs.snappy);
  await animate(
    to,
    [{ opacity: 0, transform: `translateX(${forward ? 28 : -28}px)` }, { opacity: 1, transform: "none" }],
    springs.snappy,
  );
}

function parseLink(text) {
  const match = /^sunna:\/\/([^/?#]+)[^?#]*(?:\?([^#]*))?/i.exec(text.trim());
  if (!match) return null;
  const params = new URLSearchParams(match[2] ?? "");
  return { address: decodeURIComponent(match[1]), key: params.get("key") ?? "" };
}

function iconSvg(name) {
  return `<svg class="icon"><use href="#${name}"/></svg>`;
}

function setCheckLine(line, kind, text) {
  const tone = {
    ready: "good",
    busy: "good",
    "wrong-key": "bad",
    "update-needed": "warn",
    unreachable: "warn",
    "not-found": "bad",
    invalid: "bad",
    error: "bad",
  }[kind];
  line.dataset.tone = tone ?? "";
  const icon = line.querySelector(".check-icon");
  icon.innerHTML =
    kind === "checking"
      ? '<span class="spinner"></span>'
      : tone
        ? iconSvg(tone === "good" ? "i-check" : tone === "warn" ? "i-alert" : "i-alert")
        : "";
  const span = document.createElement("span");
  span.className = "check-text";
  span.textContent = text;
  line.querySelector(".check-text").replaceWith(span);
}

function checkMessage(check, address) {
  switch (check.state) {
    case "checking":
      return `Looking for Sunna at ${hostOf(address)}…`;
    case "ready":
      return ["Found " + (check.name || hostOf(address)), check.os, check.rtt_ms != null ? ms(check.rtt_ms) : ""]
        .filter(Boolean)
        .join(" · ");
    case "busy":
      return `Found ${check.name || hostOf(address)}. ${check.viewer || "Someone"} is connected to it right now.`;
    case "unreachable":
      return `${check.detail} You can add it anyway.`;
    default:
      return check.detail || "";
  }
}

/** Add a computer, or edit one (`machine`). */
function openMachineSheet({ machine = null, opener = null, focus = "address", find = false } = {}) {
  const sheet = openSheet("machine-sheet-template", opener);
  const form = sheet.querySelector("form");
  const fields = { address: form.elements.address, key: form.elements.key, name: form.elements.name };
  const line = sheet.querySelector(".check-line");
  const submit = sheet.querySelector(".submit");
  const editing = Boolean(machine);
  sheet.querySelector("h2").textContent = editing ? machine.name : "Add a Computer";
  submit.textContent = editing ? "Save" : "Add Computer";
  sheet.querySelector(".remove").hidden = !editing;
  sheet.querySelector(".find").hidden = editing;
  fields.address.value = machine?.address ?? "";
  fields.key.value = machine?.key ?? state.defaultKey;
  fields.name.value = machine?.name ?? "";

  const preview = createPreviewTile();
  sheet.querySelector(".preview-slot").appendChild(preview);
  let latest = editing ? (state.checks.get(machine.id) ?? null) : null;
  let sequence = 0;
  let timer = 0;

  const paintPreview = () => {
    const address = fields.address.value.trim();
    const draft = {
      name: fields.name.value.trim() || latest?.name || hostOf(address) || "New computer",
      os: latest?.os || (editing ? machine.os : ""),
      device: latest?.device || (editing ? machine.device : ""),
      width: latest?.width || (editing ? machine.width : 0),
      height: latest?.height || (editing ? machine.height : 0),
      last_seen: editing ? machine.last_seen : 0,
      address,
    };
    paintTile(preview, draft, address ? (latest ?? { state: "checking" }) : { state: "idle" });
    fields.name.placeholder = latest?.name || "Optional";
  };

  const runCheck = async () => {
    const address = fields.address.value.trim();
    fields.key.closest(".input").classList.remove("error");
    if (!address) {
      latest = null;
      setCheckLine(line, "", "When a computer starts sharing, it shows its address and key.");
      paintPreview();
      return;
    }
    const mine = ++sequence;
    latest = { state: "checking" };
    setCheckLine(line, "checking", checkMessage(latest, address));
    paintPreview();
    let check;
    try {
      check = await invoke("check_machine", { address, key: fields.key.value.trim() });
    } catch (error) {
      check = { state: "invalid", detail: String(error) };
    }
    if (mine !== sequence) return;
    latest = check;
    setCheckLine(line, check.state, checkMessage(check, address));
    paintPreview();
    fields.key.closest(".input").classList.toggle("error", check.state === "wrong-key");
  };
  const schedule = () => {
    clearTimeout(timer);
    timer = setTimeout(runCheck, 420);
  };

  fields.address.addEventListener("input", () => {
    const link = parseLink(fields.address.value);
    if (link) {
      fields.address.value = link.address;
      if (link.key) fields.key.value = link.key;
      clearTimeout(timer);
      runCheck();
      return;
    }
    schedule();
  });
  fields.key.addEventListener("input", schedule);
  fields.name.addEventListener("input", paintPreview);
  sheet.querySelector(".reveal").addEventListener("click", (event) => {
    const hidden = fields.key.type === "password";
    fields.key.type = hidden ? "text" : "password";
    event.currentTarget.title = hidden ? "Hide" : "Show";
  });
  sheet.querySelector(".close").addEventListener("click", () => closeSheet(sheet));
  sheet.querySelector(".cancel").addEventListener("click", () => closeSheet(sheet));
  sheet.querySelector(".remove").addEventListener("click", async (event) => {
    if (await confirmRemove(machine, event.currentTarget)) closeSheet(sheet);
  });

  form.addEventListener("submit", async (event) => {
    event.preventDefault();
    const address = fields.address.value.trim();
    if (!address) {
      const box = fields.address.closest(".input");
      box.classList.remove("shake");
      void box.offsetWidth;
      box.classList.add("shake");
      fields.address.focus();
      return;
    }
    submit.classList.add("busy");
    const about = latest && latest.state !== "checking" ? latest : null;
    const draft = { name: fields.name.value.trim(), address, key: fields.key.value.trim(), about };
    try {
      const saved = editing
        ? await invoke("update_machine", { id: machine.id, draft })
        : await invoke("add_machine", { draft });
      if (about) state.checks.set(saved.id, about);
      if (editing) {
        state.machines = state.machines.map((entry) => (entry.id === saved.id ? saved : entry));
        renderGrid();
        closeSheet(sheet);
      } else {
        state.machines.push(saved);
        await flyIn(saved, preview, sheet);
      }
      setTimeout(poll, 300);
    } catch (error) {
      setCheckLine(line, "error", String(error));
    } finally {
      submit.classList.remove("busy");
    }
  });

  // Find on Tailscale: a second page in the same sheet.
  const formPage = sheet.querySelector(".form-page");
  const findPage = sheet.querySelector(".find-page");
  const list = sheet.querySelector(".find-list");
  const rescan = sheet.querySelector(".rescan");

  const scan = async () => {
    rescan.classList.add("spinning");
    list.replaceChildren(findMessage('<span class="spinner"></span>', "Looking on your tailnet…"));
    try {
      const result = await invoke("scan_tailscale", { key: fields.key.value.trim() || state.defaultKey });
      renderFound(list, result.machines, (found) => {
        fields.address.value = found.ip;
        if (!fields.name.value.trim()) fields.name.value = found.name;
        switchPage(sheet, findPage, formPage, false);
        runCheck();
        fields.key.focus();
      });
    } catch (error) {
      list.replaceChildren(findMessage(iconSvg("i-alert"), String(error)));
    } finally {
      setTimeout(() => rescan.classList.remove("spinning"), 400);
    }
  };
  const showFind = async () => {
    await switchPage(sheet, formPage, findPage, true);
    scan();
  };
  sheet.querySelector(".find").addEventListener("click", showFind);
  sheet.querySelector(".back").addEventListener("click", () => {
    switchPage(sheet, findPage, formPage, false);
    fields.address.focus();
  });
  rescan.addEventListener("click", scan);

  sheet.addEventListener("keydown", (event) => {
    if (event.key === "Escape" && !findPage.hidden) {
      event.stopPropagation();
      switchPage(sheet, findPage, formPage, false);
    }
  });

  if (fields.address.value) runCheck();
  else setCheckLine(line, "", "When a computer starts sharing, it shows its address and key.");
  paintPreview();
  if (find) showFind();
  else fields[focus]?.focus();
  return sheet;
}

function findMessage(iconHtml, text) {
  const message = document.createElement("div");
  message.className = "find-message";
  message.innerHTML = iconHtml;
  const span = document.createElement("span");
  span.textContent = text;
  message.appendChild(span);
  return message;
}

const FOUND_PILL = {
  ready: "Sharing",
  busy: "In use",
  "wrong-key": "Other key",
  "update-needed": "Update",
};

function renderFound(list, machines, pick) {
  const saved = new Set(state.machines.map((machine) => hostOf(machine.address)));
  const sharing = machines.filter((machine) => FOUND_PILL[machine.state]);
  const others = machines.filter((machine) => !FOUND_PILL[machine.state]);
  const item = (found, index) => {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "find-item";
    button.dataset.state = found.state;
    button.style.setProperty("--i", index);
    const already = saved.has(found.ip) || saved.has(found.name) || saved.has(found.dns_name);
    button.dataset.os = osKind(found.os);
    button.innerHTML = `<span class="os-badge"><svg class="icon"><use href="#os-screen"/></svg></span><span class="find-text"><span class="find-name"></span><span class="find-meta"></span></span><span class="pill"></span>`;
    paintMark(button.querySelector(".os-badge"), osKind(found.os));
    button.querySelector(".find-name").textContent = found.name;
    button.querySelector(".find-meta").textContent = [found.os, found.ip].filter(Boolean).join(" · ");
    const pill = button.querySelector(".pill");
    if (already) {
      pill.textContent = "Added";
      pill.classList.add("saved");
      button.disabled = true;
    } else if (FOUND_PILL[found.state]) {
      pill.textContent = FOUND_PILL[found.state];
      pill.classList.add(found.state);
      button.addEventListener("click", () => pick(found));
    } else {
      pill.textContent = found.state === "offline" ? "Offline" : "Not sharing";
      button.disabled = true;
    }
    return button;
  };
  const children = sharing.length
    ? sharing.map(item)
    : [findMessage(iconSvg("i-dots"), "No computers on your tailnet are sharing with Sunna yet.")];
  if (others.length) {
    const group = document.createElement("div");
    group.className = "find-group";
    group.textContent = `Not sharing · ${others.length}`;
    children.push(group, ...others.map((found, index) => item(found, sharing.length + index)));
  }
  list.replaceChildren(...children);
}

/** A new machine's preview flies from the sheet into its place. */
async function flyIn(machine, preview, sheet) {
  renderGrid();
  const tile = tiles.get(machine.id);
  const stage = preview.querySelector(".stage");
  const from = stage.getBoundingClientRect();
  // A copy of the preview's device, outside the sheet, flies over.
  const ghost = document.createElement("div");
  ghost.className = "tile ghost";
  for (const key of ["state", "os", "device"]) ghost.dataset[key] = preview.dataset[key] ?? "";
  ghost.toggleAttribute("data-lit", preview.hasAttribute("data-lit"));
  ghost.style.cssText = preview.style.cssText;
  ghost.appendChild(stage.cloneNode(true));
  const frame = (rect) => ({ left: `${rect.left}px`, top: `${rect.top}px`, width: `${rect.width}px` });
  Object.assign(ghost.style, frame(from));
  document.body.appendChild(ghost);
  tile.classList.add("arriving");
  tile.scrollIntoView({ block: "nearest" });
  closeSheet(sheet);
  const to = restingRect(tile.querySelector(".stage"));
  await animate(ghost, [frame(from), frame(to)], springs.gentle, { fill: "forwards" });
  tile.classList.remove("arriving");
  ghost.remove();
}

/** Ask, then remove: the tile fades away and the rest close the gap. */
async function confirmRemove(machine, opener) {
  const sheet = openSheet("confirm-template", opener);
  sheet.querySelector("h2").textContent = `Remove ${machine.name}?`;
  sheet.querySelector(".confirm-body").textContent =
    "It leaves this list. You can add it again any time with its address and key.";
  sheet.querySelector(".cancel").focus();
  return new Promise((resolve) => {
    sheet.querySelector(".cancel").addEventListener("click", async () => {
      await closeSheet(sheet);
      resolve(false);
    });
    sheet.querySelector(".confirm").addEventListener("click", async () => {
      await closeSheet(sheet);
      resolve(true);
      await removeMachine(machine);
    });
    sheet.addEventListener("cancel-sheet", () => resolve(false), { once: true });
  });
}

async function removeMachine(machine) {
  try {
    await invoke("remove_machine", { id: machine.id });
  } catch (error) {
    toast(String(error));
    return;
  }
  const tile = tiles.get(machine.id);
  if (tile) {
    tile.classList.add("leaving");
    await wait(380);
  }
  const grid = $("grid");
  const before = snapshot([...grid.children].filter((element) => element !== tile));
  state.machines = state.machines.filter((entry) => entry.id !== machine.id);
  state.checks.delete(machine.id);
  renderGrid();
  flip(before);
  toast(`Removed ${machine.name}.`, "good");
}

// ───────────── Settings ─────────────

/** The System shortcuts row: whether macOS lets Sunna send ⌘Tab and the like,
 * kept current while the sheet is open (it changes in System Settings). */
function wireShortcutsRows(sheet) {
  if (state.info?.platform !== "macos") return;
  const row = sheet.querySelector(".shortcuts-row");
  const allowed = row.querySelector(".access-state");
  const allow = row.querySelector(".access-allow");
  const resetRow = sheet.querySelector(".access-reset-row");
  row.hidden = false;
  let asked = false;
  const refresh = async () => {
    if (!sheet.isConnected) return clearInterval(timer);
    const granted = await invoke("keyboard_access").catch(() => true);
    allowed.hidden = !granted;
    allow.hidden = granted;
    // Asked, but still no: most likely an entry left from an older build.
    resetRow.hidden = granted || !asked;
  };
  const timer = setInterval(refresh, 1500);
  refresh();
  allow.addEventListener("click", async () => {
    asked = true;
    await invoke("allow_keyboard_access").catch((error) => toast(String(error)));
    setTimeout(refresh, 800);
  });
  sheet.querySelector(".access-reset").addEventListener("click", async () => {
    try {
      await invoke("reset_keyboard_access");
    } catch (error) {
      toast(String(error), "info");
    }
    setTimeout(refresh, 800);
  });
}

async function openSettings(opener) {
  if (sheets.some((entry) => entry.sheet.classList.contains("settings-sheet"))) return;
  const sheet = openSheet("settings-sheet-template", opener);
  sheet.querySelector(".close").addEventListener("click", () => closeSheet(sheet));
  let settings;
  try {
    settings = await invoke("get_settings");
  } catch (error) {
    toast(String(error));
    return;
  }
  const key = sheet.querySelector('input[name="key"]');
  key.value = settings.key;
  let saveTimer = 0;
  const save = (delay = 0) => {
    clearTimeout(saveTimer);
    saveTimer = setTimeout(async () => {
      try {
        await invoke("set_settings", { settings });
        state.defaultKey = settings.key;
      } catch (error) {
        toast(String(error));
      }
    }, delay);
  };
  key.addEventListener("input", () => {
    settings.key = key.value.trim();
    save(500);
  });
  sheet.querySelector(".reveal").addEventListener("click", () => {
    key.type = key.type === "password" ? "text" : "password";
  });
  sheet.querySelector(".copy-key").addEventListener("click", (event) => copy(key.value, event.currentTarget));

  const stats = sheet.querySelector('input[name="stats"]');
  const menuButton = sheet.querySelector('input[name="menu_button"]');
  const audio = sheet.querySelector('input[name="audio"]');
  stats.checked = settings.stats;
  menuButton.checked = settings.menu_button;
  audio.checked = settings.audio !== false;
  stats.addEventListener("change", () => {
    settings.stats = stats.checked;
    save();
  });
  menuButton.addEventListener("change", () => {
    settings.menu_button = menuButton.checked;
    save();
  });
  audio.addEventListener("change", () => {
    settings.audio = audio.checked;
    save();
  });
  const segments = sheet.querySelectorAll(".segmented button");
  const paintSegments = () => {
    for (const button of segments) {
      button.setAttribute("aria-checked", String((button.dataset.value === "window") === settings.windowed));
    }
  };
  paintSegments();
  for (const button of segments) {
    button.addEventListener("click", () => {
      settings.windowed = button.dataset.value === "window";
      paintSegments();
      save();
    });
  }

  wireShortcutsRows(sheet);
  const info = await invoke("app_info").catch(() => ({}));
  sheet.querySelector(".about").textContent = `Sunna ${info.version ?? ""} · protocol ${info.protocol ?? ""}`;
  sheet.querySelector(".this-name").textContent = info.computer || "This computer";
  const me = await invoke("this_computer").catch(() => null);
  if (!me || !sheet.isConnected) return;
  const sharing = sheet.querySelector(".this-sharing");
  sharing.dataset.state = me.sharing;
  sheet.querySelector(".this-sharing-text").textContent =
    {
      ready: `Sharing at ${me.address}`,
      busy: `Sharing at ${me.address} · in a session`,
      "wrong-key": "Sharing, but with a different key than the one below",
      unknown: "Not on a Tailscale network",
    }[me.sharing] ??
    (info.platform === "linux"
      ? "Not sharing. Run sunna-host setup to share it."
      : "Not sharing. To share this Mac, run scripts/run.sh host.");
  if (me.address && me.key) {
    const link = `sunna://${me.address}?key=${me.key}`;
    const row = sheet.querySelector(".link-row");
    row.hidden = false;
    row.querySelector(".link-text").textContent = `sunna://${me.address}?key=••••`;
    row.querySelector(".copy-link").addEventListener("click", (event) => copy(link, event.currentTarget));
    row.querySelector(".show-qr").addEventListener("click", (event) => openQrSheet(me, event.currentTarget));
  }
}

async function openQrSheet(machine, opener) {
  const sheet = openSheet("qr-sheet-template", opener);
  sheet.querySelector("h2").textContent = `Add “${machine.name}” on a Phone`;
  sheet.querySelector(".close").addEventListener("click", () => closeSheet(sheet));
  const done = sheet.querySelector(".done");
  done.addEventListener("click", () => closeSheet(sheet));
  done.focus();
  try {
    const svg = await invoke("qr_code", { text: `sunna://${machine.address}?key=${machine.key}` });
    if (!sheet.isConnected || topSheet() !== sheet) return;
    const card = sheet.querySelector(".qr-card");
    card.innerHTML = svg;
    card.setAttribute("aria-busy", "false");
  } catch {
    if (!sheet.isConnected || topSheet() !== sheet) return;
    closeSheet(sheet);
    toast("Couldn't make the QR code.");
  }
}

// ───────────── Menu ─────────────

let menuAnchor = null;

function openMenu(id, { x, y, anchor }) {
  const machine = byId(id);
  if (!machine || state.session) return;
  hideMenu();
  const ready = state.checks.get(id)?.state === "ready";
  const menu = $("menu");
  const item = (label, icon, shortcut, run, { disabled = false, danger = false } = {}) => {
    const button = document.createElement("button");
    button.type = "button";
    button.className = `menu-item${danger ? " danger" : ""}`;
    button.setAttribute("role", "menuitem");
    button.disabled = disabled;
    button.innerHTML = `${iconSvg(icon)}<span></span><span class="menu-shortcut">${shortcut}</span>`;
    button.querySelector("span").textContent = label;
    button.addEventListener("click", () => {
      hideMenu();
      run();
    });
    return button;
  };
  const separator = document.createElement("div");
  separator.className = "menu-separator";
  const tile = tiles.get(id);
  menu.replaceChildren(
    item("Connect", "i-arrow", "↵", () => connect(machine), { disabled: !ready }),
    item("Edit…", "i-edit", "⌘E", () => openMachineSheet({ machine, opener: tile })),
    item("Copy Address", "i-copy", "", () => copy(machine.address)),
    item("Show QR Code", "i-qr", "", () => openQrSheet(machine, tile)),
    separator,
    item("Remove…", "i-trash", "⌫", () => confirmRemove(machine, tile), { danger: true }),
  );
  menu.hidden = false;
  const size = menu.getBoundingClientRect();
  let left = x;
  let top = y;
  if (anchor) {
    const rect = anchor.getBoundingClientRect();
    left = rect.right - size.width;
    top = rect.bottom + 6;
    menuAnchor = anchor;
    anchor.setAttribute("aria-expanded", "true");
  }
  left = Math.max(8, Math.min(left, innerWidth - size.width - 8));
  top = Math.max(8, Math.min(top, innerHeight - size.height - 8));
  menu.style.left = `${left}px`;
  menu.style.top = `${top}px`;
  const originX = anchor ? size.width - 14 : 0;
  menu.style.transformOrigin = `${originX}px 0`;
  animate(menu, [{ opacity: 0, transform: "scale(0.9)" }, { opacity: 1, transform: "none" }], springs.bouncy);
  menu.querySelector(".menu-item:not(:disabled)")?.focus({ preventScroll: true });
}

function hideMenu() {
  const menu = $("menu");
  if (menu.hidden) return;
  menu.hidden = true;
  menuAnchor?.setAttribute("aria-expanded", "false");
  menuAnchor = null;
}

document.addEventListener("pointerdown", (event) => {
  if (!$("menu").hidden && !$("menu").contains(event.target)) hideMenu();
});
addEventListener("blur", hideMenu);
addEventListener("resize", hideMenu);

// ───────────── Toast, copy ─────────────

let toastTimer = 0;
function toast(text, kind = "error", action = null) {
  const element = $("toast");
  $("toast-text").textContent = text;
  element.className = `toast ${kind}`;
  element.querySelector(".toast-icon use").setAttribute("href", kind === "good" ? "#i-check" : "#i-alert");
  const button = $("toast-action");
  button.hidden = !action;
  button.textContent = action?.label ?? "";
  button.onclick = action
    ? () => {
        hideToast();
        action.run();
      }
    : null;
  const wasHidden = element.hidden;
  element.hidden = false;
  if (wasHidden) {
    animate(
      element,
      [{ opacity: 0, transform: "translateY(14px) scale(0.96)" }, { opacity: 1, transform: "none" }],
      springs.bouncy,
    );
  }
  clearTimeout(toastTimer);
  toastTimer = setTimeout(hideToast, kind === "error" ? 8000 : 4500);
}

async function hideToast() {
  const element = $("toast");
  if (element.hidden) return;
  clearTimeout(toastTimer);
  await element
    .animate([{ opacity: 1 }, { opacity: 0, transform: "translateY(8px)" }], { duration: 160, fill: "forwards" })
    .finished.catch(() => {});
  element.hidden = true;
  for (const animation of element.getAnimations()) animation.cancel();
}

async function copy(text, button = null) {
  try {
    await navigator.clipboard.writeText(text);
  } catch {
    toast("Couldn't copy. Select the text and press ⌘C.", "info");
    return;
  }
  if (!button) {
    toast("Copied.", "good");
    return;
  }
  const use = button.querySelector("use");
  const before = use.getAttribute("href");
  use.setAttribute("href", "#i-check");
  button.classList.add("done");
  animate(button, [{ transform: "scale(0.8)" }, { transform: "none" }], springs.bouncy);
  setTimeout(() => {
    use.setAttribute("href", before);
    button.classList.remove("done");
  }, 1300);
}

// ───────────── Keyboard ─────────────

function focusedMachine() {
  const tile = document.activeElement?.closest?.(".tile[data-id]");
  return tile ? byId(tile.dataset.id) : null;
}

document.addEventListener("keydown", (event) => {
  const command = event.metaKey || event.ctrlKey;
  const sheet = topSheet();

  if (!$("menu").hidden) {
    const items = [...$("menu").querySelectorAll(".menu-item:not(:disabled)")];
    const index = items.indexOf(document.activeElement);
    if (event.key === "Escape") {
      hideMenu();
      event.preventDefault();
    } else if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      const step = event.key === "ArrowDown" ? 1 : -1;
      items[(index + step + items.length) % items.length]?.focus();
      event.preventDefault();
    }
    return;
  }

  if (state.session) {
    if (event.key === "Escape") invoke("cancel_connect");
    return;
  }

  if (sheet) {
    if (event.key === "Escape") {
      event.preventDefault();
      sheet.dispatchEvent(new Event("cancel-sheet"));
      closeSheet(sheet);
    } else if (event.key === "Tab") {
      const items = focusables(sheet);
      if (!items.length) return;
      const index = items.indexOf(document.activeElement);
      const next = event.shiftKey ? (index <= 0 ? items.length - 1 : index - 1) : (index + 1) % items.length;
      items[next].focus();
      event.preventDefault();
    }
    return;
  }

  if (command && event.key.toLowerCase() === "n") {
    event.preventDefault();
    openMachineSheet({ opener: $("add-button") });
  } else if (command && event.key === ",") {
    event.preventDefault();
    openSettings($("settings-button"));
  } else if (command && event.key.toLowerCase() === "r") {
    event.preventDefault();
    poll();
  } else if (command && /^[1-9]$/.test(event.key)) {
    const machine = state.machines[Number(event.key) - 1];
    if (machine) {
      event.preventDefault();
      activate(machine.id);
    }
  } else if (command && event.key.toLowerCase() === "e" && focusedMachine()) {
    event.preventDefault();
    const machine = focusedMachine();
    openMachineSheet({ machine, opener: tiles.get(machine.id) });
  } else if ((event.key === "Backspace" || event.key === "Delete") && focusedMachine()) {
    event.preventDefault();
    const machine = focusedMachine();
    confirmRemove(machine, tiles.get(machine.id));
  } else if (event.key.startsWith("Arrow") && document.activeElement?.closest?.("#grid")) {
    const hits = [...$("grid").querySelectorAll(".tile-hit")];
    const index = hits.indexOf(document.activeElement);
    if (index < 0) return;
    const columns = getComputedStyle($("grid")).gridTemplateColumns.split(" ").length;
    const step = { ArrowRight: 1, ArrowLeft: -1, ArrowDown: columns, ArrowUp: -columns }[event.key];
    const next = hits[index + step];
    if (next) {
      event.preventDefault();
      next.focus();
    }
  } else if ((event.key === "ContextMenu" || (event.shiftKey && event.key === "F10")) && focusedMachine()) {
    event.preventDefault();
    const machine = focusedMachine();
    openMenu(machine.id, { anchor: tiles.get(machine.id).querySelector(".tile-more") });
  }
});

// ───────────── Start ─────────────

$("add-button").addEventListener("click", (event) => openMachineSheet({ opener: event.currentTarget }));
$("settings-button").addEventListener("click", (event) => openSettings(event.currentTarget));
$("welcome-add").addEventListener("click", (event) => openMachineSheet({ opener: event.currentTarget }));
$("welcome-find").addEventListener("click", (event) =>
  openMachineSheet({ opener: event.currentTarget, find: true }),
);

async function start() {
  renderHeader();
  // The toolbar gets a backing once the computers scroll under it.
  $("home").addEventListener("scroll", () => {
    document.querySelector(".toolbar").classList.toggle("scrolled", $("home").scrollTop > 2);
  });
  const [info, settings, machines] = await Promise.all([
    invoke("app_info").catch(() => ({})),
    invoke("get_settings").catch(() => ({ key: "" })),
    invoke("list_machines").catch((error) => {
      toast(String(error));
      return [];
    }),
  ]);
  state.info = info;
  // Linux draws its own title bar: no room to leave for traffic lights.
  document.documentElement.dataset.platform = info.platform ?? "";
  state.defaultKey = settings.key ?? "";
  state.machines = machines;
  state.loaded = true;
  renderGrid();
  setTimeout(() => $("grid").classList.add("settled"), 1600);
  // Without Accessibility, ⌘Tab and friends stay on this Mac: say so once.
  if (info.platform === "macos" && !(await invoke("keyboard_access").catch(() => true))) {
    toast("⌘Tab, ⌘Space and other shortcuts will stay on this Mac.", "info", {
      label: "Allow…",
      run: () => invoke("allow_keyboard_access"),
    });
  }
  poll();
  setInterval(poll, POLL_MS);
  document.addEventListener("visibilitychange", () => {
    if (!document.hidden) poll();
  });
  addEventListener("focus", poll);
}

start();
