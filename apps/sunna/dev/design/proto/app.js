/* Sunna prototype: local tab state; no host is contacted.
 * Data, screen rendering and status: guide §§2–3, 9.
 * Springs and navigation (go/back): §§1, 4.
 * Stage/action and overview (V): §§5–6; form/discovery/QR: §7.
 * Settings/permissions: §9; sessions, menus and connection flow: §8.
 * Keyboard, gestures and safe-area layout: §10. Fixtures live in test.js.
 */
"use strict";
const $ = (s, r = document) => r.querySelector(s),
  $$ = (s, r = document) => [...r.querySelectorAll(s)];
const app = $("#app"),
  content = $("#content"),
  phone = document.body.classList.contains("phone");
const params = new URLSearchParams(location.search);
const BASE = [
  {
    id: "arch",
    frame: "arch",
    name: "archlinux",
    os: "Arch Linux",
    kind: "Desktop",
    w: 2560,
    h: 1440,
    state: "ready",
    ms: 12,
  },
  {
    id: "mac",
    frame: "mac",
    name: "MacBook Air",
    os: "macOS 26",
    kind: "Laptop",
    w: 2940,
    h: 1912,
    state: "ready",
    ms: 7,
  },
  {
    id: "studio",
    frame: "studio",
    name: "studio",
    os: "Fedora Linux 42",
    kind: "Desktop",
    w: 3440,
    h: 1440,
    state: "busy",
    by: "Priya's MacBook Pro",
    ms: 16,
  },
  {
    id: "ubuntu",
    frame: "ubuntu",
    name: "ubuntu",
    os: "Ubuntu 22.04",
    kind: "VM",
    w: 1920,
    h: 1080,
    state: "key",
    ms: 38,
  },
  {
    id: "gaming",
    frame: "gaming",
    name: "gaming-pc",
    os: "Windows 11",
    kind: "Desktop",
    w: 2560,
    h: 1440,
    state: "offline",
    ms: 9,
    seen: "3 h ago",
  },
];
const STATES = [
  "loading",
  "idle",
  "checking",
  "ready-no-latency",
  "ready",
  "busy-anonymous",
  "busy",
  "key",
  "update",
  "update-client",
  "not-found",
  "invalid",
  "unreachable",
  "offline",
  "never",
];
let C = [],
  sel = 0,
  cur = "computer",
  timer = 0,
  checkTimer = 0,
  sessionEpoch = 0,
  noticeTimer = 0,
  toastTimer = 0,
  motion = null;
let suppressHistory = false,
  idleTimer = 0;
let form = { address: "", key: "", name: "", state: "idle", revealed: false, link: false };
const cfg = {
  stats: !phone,
  sound: true,
  menu: true,
  capture: true,
  clipboard: true,
  fullscreen: true,
  openSessions: "Full screen",
  keyboardButton: true,
  suggestions: false,
  touch: "Trackpad",
  codec: phone ? "Auto" : "HEVC (sharper per bit)",
  resolution: phone ? "Full" : "Native",
  fps: phone ? "60" : "60 fps",
  bitrate: phone ? "Auto" : "20 Mbps",
  fast: true,
  remember: true,
  previews: true,
  ownName: "Pixel 9",
  ownKey: "sunna-example-key",
};
let ui = {
  context: false,
  remove: false,
  fail: "",
  ended: "",
  nextFail: "success",
  notice: "",
  phase: "reaching",
  submenu: "",
  permission: "allowed",
  self: "ready",
  qr: "ready",
  discovery: "results",
  test: false,
  reduced: false,
  still: params.get("still") === "1",
  keyboard: false,
  modifier: {},
  statsDetail: "full",
  decoder: "hardware",
  pointer: "remote",
  zoom: 1,
  toast: "",
  saveError: false,
  removeError: false,
  streamError: false,
  notification: "connected",
  scan: false,
  permissionShown: false,
  licensesEmpty: false,
  idle: false,
  floatX: undefined,
  menuHover: false,
};
const FAILS = {
  unexpected: "The session ended unexpectedly.",
  crashed: "The viewer crashed. Its log has the details.",
  timeout:
    "Lost the connection: nothing came back from that computer for 10 seconds. Its network dropped, or it went to sleep.",
  host: "That computer ended the session: it restarted, or stopped sharing.",
  replaced: `This session moved to a newer one from this ${phone ? "phone" : "Mac"}.`,
  key: "The key doesn't match. Edit the computer and paste its key.",
  busy: "Priya is connected to that computer right now.",
  "busy-anonymous": "Someone else is connected to that computer.",
  decoder: `This ${phone ? "phone" : "computer"} can't play that computer's video (no HEVC decoder).`,
  version: "That computer runs a different version of Sunna. Update both.",
  unreachable: phone
    ? "Couldn't reach 100.101.102.103. Is Sunna sharing on that computer?"
    : "Couldn't reach that computer (connection refused).",
  unanswered: "Nothing answered at 100.101.102.103 for 10 seconds. Is Sunna sharing on that computer?",
  spawn: "Couldn't start the viewer: executable not found",
  already: "A session is already open.",
  missing: "That computer is no longer in your list.",
  resolution: "Couldn't find a computer called “workstation”.",
  invalid: "That doesn't look like an address.",
  raw: "host pipeline failed: capture unavailable",
  cancelled: "Disconnected",
};
const NOTICES = {
  help: phone
    ? "The screen is a trackpad: tap to click, two fingers to scroll. The buttons at the top: the keyboard, and ••• for the menu."
    : "••• (top right) or ⌃⌥M: menu  ·  ⌃⌥G release keyboard  ·  ⌃⌥F full screen  ·  ⌃⌥Q disconnect",
  unavailable:
    "⌘Tab, ⌘Space and other shortcuts stay on this Mac: allow them in Sunna's Settings → System shortcuts, then reconnect  ·  ⌃⌥Q disconnect",
  captured:
    "Keyboard captured: ⌘Tab, ⌘Space and other shortcuts go to the remote  ·  ⌃⌥G gives them back to this Mac",
  released: "Keyboard released: shortcuts stay on this Mac  ·  ⌃⌥G to capture again",
  changing: "Changing the stream…",
  changed: "Now streaming HEVC 2560×1440  ·  fast lane on",
  "stream-error": phone
    ? "The computer kept its video as it was: requested codec unavailable"
    : "Couldn't change the stream: requested codec unavailable",
  "clipboard-on": "Clipboard sharing on",
  "clipboard-off": "Clipboard sharing paused: copies stay on each computer",
  "clipboard-empty": `The clipboard has no text to type${phone ? "." : ""}`,
  typing: "Typing 24 characters",
  skipped: `Typing 24 characters${phone ? " · " : "  ·  "}2 skipped (not on a US keyboard)`,
  sent: "Sent Escape",
  "sound-on": "Sound on",
  "sound-off": "Sound off",
  hidden: "Menu button hidden  ·  ⌃⌥M opens the menu",
  "no-drawing": "No picture from archlinux: its screen isn't being drawn",
  "no-video": "No picture: the video isn't getting through",
  unsupported: "Some characters can't be typed: Sunna types on a US keyboard layout.",
  touch: "Touch: tap where you want to click. Touch and hold for a right click.",
  trackpad: "Trackpad: drag to move the pointer, tap to click, two fingers to scroll.",
  link: "Leave the session to add a computer.",
  copied: phone ? "Copied 100.101.102.103" : "Copied.",
  storage: "Couldn't save it: the phone's storage may be full.",
  action: "archlinux isn't responding.",
  normal: "Copied.",
};
const MAC_KEYS = [
  "Switch Apps (⌘Tab)",
  "Spotlight (⌘Space)",
  "Mission Control (⌃↑)",
  "Force Quit… (⌥⌘Esc)",
  "Lock Screen (⌃⌘Q)",
  "Escape",
];
const LINUX_KEYS = [
  "Switch Windows (Alt+Tab)",
  "App Launcher (Super)",
  "Terminal (Ctrl+Alt+T)",
  "Lock Screen (Super+L)",
  "Log Out… (Ctrl+Alt+Delete)",
  "Escape",
];
const icons = {
  grid: '<rect x="3" y="3" width="6" height="6" rx="1"/><rect x="13" y="3" width="6" height="6" rx="1"/><rect x="3" y="13" width="6" height="6" rx="1"/><rect x="13" y="13" width="6" height="6" rx="1"/>',
  add: '<path d="M11 3v16M3 11h16"/>',
  settings:
    '<path d="M3 6h8m5 0h3M3 16h3m5 0h8"/><circle cx="13.5" cy="6" r="2.5"/><circle cx="8.5" cy="16" r="2.5"/>',
  back: '<path d="m14 4-7 7 7 7"/>',
};
function icon(n) {
  return `<svg viewBox="0 0 22 22" width="16" height="16" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round">${icons[n] || ""}</svg>`;
}
function mark(c) {
  return /mac/i.test(c.os)
    ? '<span class="system-mark">⌘</span>'
    : /windows/i.test(c.os)
      ? '<svg class="system-mark" viewBox="0 0 20 20" fill="none" stroke="currentColor"><rect x="3" y="3" width="14" height="14" rx="1"/><path d="M10 3v14M3 10h14"/></svg>'
      : /android/i.test(c.os)
        ? '<svg class="system-mark" viewBox="0 0 20 20" fill="none" stroke="currentColor"><rect x="5" y="1" width="10" height="18" rx="2"/><path d="M8 16h4"/></svg>'
        : '<img class="system-mark" src="tux.svg" alt="Linux">';
}
function makeComputers(n) {
  C = Array.from({ length: n }, (_, i) => ({
    ...BASE[i % 5],
    id: i < 5 ? BASE[i].id : "computer-" + i,
    name:
      i < 5
        ? BASE[i].name
        : ["workstation", "living-room", "build-server", "travel-laptop", "study", "Pixel tablet", "office"][
            i - 5
          ] || "computer-" + i,
    state: i < 5 ? BASE[i].state : i === 10 ? "never" : "ready",
    os: i === 10 ? "Android 16" : BASE[i % 5].os,
    kind: i === 10 ? "Tablet" : BASE[i % 5].kind,
    address: `100.101.102.${103 + i}`,
    key: "sunna-example-key",
  }));
  sel = Math.max(0, Math.min(sel, C.length - 1));
}
function current() {
  return C[sel] || { ...BASE[0], name: "New computer", state: "idle", id: "new", address: form.address };
}
function statusText(c) {
  return (
    {
      loading: "Looking for your computers…",
      idle: "Enter its address",
      checking: "Looking…",
      "ready-no-latency": "Ready",
      ready: `Ready · ${c.ms || 12} ms`,
      "busy-anonymous": "In use",
      busy: `In use · ${c.by || "Priya"}`,
      key: "Key doesn't match",
      update: "Needs an update",
      "update-client": "Needs an update",
      "not-found": "Address not found",
      invalid: "Address isn't valid",
      unreachable: "Not responding",
      offline: `Offline · seen ${c.seen || "3 h ago"}`,
      never: "Ready",
    }[c.state] || c.state
  );
}
function ready(c) {
  return ["ready", "ready-no-latency", "never"].includes(c.state);
}
function shot(c, live = false) {
  let s = live ? "ready" : c.state,
    never = ["idle", "invalid", "not-found", "unreachable", "never"].includes(s) || c.never;
  if (s === "offline" && !cfg.remember) never = true;
  let cls =
    ["ready", "ready-no-latency", "busy", "busy-anonymous"].includes(s) && cfg.previews ? "live " : "";
  return `<div class="shot ${cls}s-${s}" data-shot="${esc(c.id)}" style="aspect-ratio:${c.w}/${c.h}">${never ? `<div class="d never">${mark(c)}<span>${esc(c.name)}</span></div>` : SCREEN[c.frame || "arch"]()}${s === "key" ? LOCK : ""}${s.startsWith("update") ? '<div class="update-mark">↑</div>' : ""}${s.startsWith("busy") ? '<span class="tag"><i class="dot busy"></i>In use</span>' : ""}</div>`;
}
function stat(c, extra = false) {
  return `<div class="stat"><i class="dot ${c.state}"></i><span class="tx">${esc(statusText(c))}</span>${extra ? `<em>· ${esc(c.os)} · ${esc(c.kind)} · ${c.w} × ${c.h}</em>` : ""}</div>`;
}
function emptyShot() {
  return '<div class="shot empty-screen" data-shot="new" style="aspect-ratio:16/9"><span><b>＋</b>Add a computer</span></div>';
}
function btn(label, action, cls = "", attrs = "") {
  return `<button class="${cls}" data-action="${action}" ${attrs}>${label}</button>`;
}
function toolbar(back = false) {
  return `<header class="toolbar">${phone ? "" : '<div class="window-buttons" aria-hidden="true"><i></i><i></i><i></i></div>'}${btn(back ? "‹ Computers" : icon("grid") + "All computers", back ? "home" : "overview", "overview-button")}<div class="tools">${back ? "" : btn(icon("add"), "add", "icon", 'aria-label="Add a computer" title="Add a computer (⌘N)"') + btn(icon("settings"), "settings", "icon", 'aria-label="Settings" title="Settings (⌘,)"')}</div></header>`;
}
// Copied damped spring from directions.html, sampled into CSS linear().
function spring(k, c) {
  let x = 0,
    v = 0,
    t = 0;
  const dt = 1 / 240,
    pts = [0];
  while (t < 2.5) {
    v += (-k * (x - 1) - c * v) * dt;
    x += v * dt;
    t += dt;
    pts.push(x);
    if (Math.abs(x - 1) < 0.0006 && Math.abs(v) < 0.006) break;
  }
  const out = [];
  for (let i = 0; i <= 48; i++) out.push(+pts[Math.round((i / 48) * (pts.length - 1))].toFixed(4));
  out[48] = 1;
  return { easing: `linear(${out.join(",")})`, duration: Math.round(t * 1000) };
}
const SP = { move: spring(190, 24), zoom: spring(150, 22) };
app.style.setProperty("--sp", SP.move.easing);
app.style.setProperty("--spd", SP.move.duration + "ms");
function reduced() {
  return ui.reduced || matchMedia("(prefers-reduced-motion:reduce)").matches;
}
function box(el) {
  const a = el.getBoundingClientRect(),
    b = app.getBoundingClientRect();
  return { x: a.left - b.left, y: a.top - b.top, w: a.width, h: a.height };
}
function animate(el, frames, type = "zoom") {
  if (!el || ui.still) return;
  return el.animate(
    reduced() ? { opacity: [0.2, 1] } : frames,
    reduced() ? { duration: 150, easing: "linear" } : { ...SP[type] },
  );
}
function source() {
  return (
    $(".sess-shot .shot") ||
    $(".st-slide.on .shot") ||
    $(`.wt[data-i="${sel}"] .shot`) ||
    $(".form-screen .shot")
  );
}
let closingMenu = false;
const parents = {};
function go(v, opts = {}) {
  if (closingMenu) return;
  if (cur === "menu" && v !== "menu" && !ui.still && !opts.afterFade) {
    closingMenu = true;
    const fades = $$(".session-controls,.submenu").map(
      (el) =>
        el.animate(
          { opacity: [getComputedStyle(el).opacity, 0] },
          { duration: reduced() ? 150 : 200, fill: "forwards" },
        ).finished,
    );
    Promise.all(fades).then(() => {
      closingMenu = false;
      go(v, { ...opts, afterFade: true });
    });
    return;
  }
  if (
    !opts.back &&
    v !== (parents[cur] || { find: "add", licenses: "settings" }[cur]) &&
    v !== cur &&
    ["settings", "add", "edit", "find", "qr", "licenses"].includes(v)
  )
    parents[v] = cur;
  const from = opts.from || source(),
    r = from ? box(from) : null;
  if (!suppressHistory && v !== cur) history.pushState({ view: v }, "");
  cur = v;
  ui.context = false;
  render();
  const to = opts.to ? $(opts.to) : source();
  if (r && to && r.w) {
    const d = box(to);
    if (d.w) {
      motion = animate(
        to,
        [
          {
            transform: `translate(${r.x - d.x}px,${r.y - d.y}px) scale(${r.w / d.w},${r.h / d.h})`,
            transformOrigin: "0 0",
          },
          { transform: "none", transformOrigin: "0 0" },
        ],
        opts.move ? "move" : "zoom",
      );
    }
  }
  if (!ui.still) {
    for (const el of $$(".st-info,.act,.st-tabs,.toolbar,.session-controls,.form-column,.settings-column"))
      el.animate(
        [
          { opacity: 0, transform: "scale(.97)" },
          { opacity: 1, transform: "none" },
        ],
        {
          duration: reduced() ? 150 : 200,
          delay: cur === "menu" ? 120 : 0,
          fill: "backwards",
          easing: "linear",
        },
      );
  }
  opts.onStart?.();
  updateTest();
}
function dimensions() {
  return {
    W: app.clientWidth,
    H: app.clientHeight,
    TOP: parseFloat(getComputedStyle($(".view") || app).top) || 52,
  };
}
function action(c) {
  if (ui.remove)
    return `<div class="act reason"><p>Remove ${phone ? "“" : ""}${esc(c.name)}${phone ? "”" : ""}?</p><div class="row">${btn("Remove", "confirm-remove", "danger")}${btn("Keep", "keep", "secondary")}</div></div>`;
  if (ui.fail || ui.ended)
    return `<div class="act reason"><p>${esc(FAILS[ui.fail || ui.ended] || ui.fail || ui.ended)}</p>${btn(ui.ended ? "Reconnect" : "Try again", "connect", "primary")}</div>`;
  let a = ready(c)
    ? btn("Connect" + (phone ? "" : "<kbd>↵</kbd>"), "connect", "primary")
    : c.state.startsWith("busy")
      ? "<p>You can connect when they leave.</p>"
      : c.state === "key"
        ? '<form class="keyf" id="key-fix"><input type="password" placeholder="Paste its new key" aria-label="New key"><button class="secondary">Update key</button></form>'
        : c.state === "offline"
          ? `<p>Shown as it was ${esc(c.seen || "3 h ago")}. It lights up when it's back.</p>`
          : c.state.startsWith("update")
            ? `<p>${c.state === "update-client" ? "That computer runs a newer Sunna. Update this one." : "That computer runs an older Sunna. Update it."}</p>`
            : c.state === "checking" || c.state === "loading"
              ? `<p>Still looking for ${esc(c.name)}…</p>`
              : `<p>${esc(detail(c.state, c.address))}</p>${btn("Edit", "edit", "secondary")}`;
  return `<div class="act${!ready(c) && c.state !== "key" ? " reason" : ""}">${a}</div>`;
}
function contextMenu() {
  let c = current();
  return `<div class="context" role="menu">${!phone || ready(c) ? btn("Connect<kbd>↵</kbd>", "connect", "", ready(c) ? "" : "disabled") : ""}${btn("Edit…<kbd>⌘E</kbd>", "edit")}${btn("Copy address", "copy-address")}${btn("Copy link", "copy-link")}${!phone ? btn("Show QR code", "qr") : ""}${btn("Remove…<kbd>⌫</kbd>", "remove", "danger")}</div>`;
}
const V = {
  stage: {
    render() {
      const { W, H, TOP } = dimensions(),
        vh = H - TOP;
      let h = phone ? Math.min(245, vh * 0.34) : Math.min(560, Math.max(240, vh * 0.58));
      if (!phone) h = Math.min(h, Math.max(240, vh - 310), (W - 130) / (current().w / current().h));
      const landscape = phone && H < 500;
      const widths = C.map((c) => (phone ? (landscape ? W * 0.44 : W - 48) : (h * c.w) / c.h));
      widths.push(phone ? (landscape ? W * 0.44 : W - 48) : (h * 16) / 9);
      const top = phone ? (landscape ? 40 : Math.max(42, vh * 0.1)) : 26;
      const gap = phone ? 14 : 44;
      const center = landscape ? W * 0.25 : W / 2;
      const offset = center - widths.slice(0, sel).reduce((s, w) => s + w + gap, 0) - widths[sel] / 2;
      const pictureH = phone ? (widths[sel] * current().h) / current().w : h;
      const infoTop = top + (phone ? Math.max(pictureH, landscape ? 0 : 180) : h) + 28;
      content.innerHTML =
        toolbar() +
        `<section class="view"><div class="st" style="--stage-top:${top}px;--info-top:${infoTop}px"><div class="st-track" style="transform:translateX(${offset}px)">${C.map((c, i) => `<div class="st-slide ${i === sel ? "on" : ""}" data-i="${i}" style="width:${widths[i]}px">${shot(c)}</div>`).join("")}<div class="st-slide" data-i="${C.length}" style="width:${widths.at(-1)}px">${emptyShot()}</div></div><div class="st-info"><div class="name-row"><h1 title="${esc(current().name)}">${esc(current().name)}</h1>${btn("•••", "context", "icon more-computer", 'aria-label="More"')}${ui.context ? contextMenu() : ""}</div>${stat(current(), !phone)}${phone ? `<p class="system-line">${mark(current())} ${esc(current().os)} · ${esc(current().kind)}<br>${current().w} × ${current().h}</p>` : ""}</div>${action(current())}<nav class="st-tabs" aria-label="Computers"><div class="in">${C.map((c, i) => `<button data-select="${i}" class="${i === sel ? "on" : ""}" aria-label="${esc(c.name)}"><i class="dot ${c.state}"></i><span>${esc(c.name)}</span></button>`).join("")}${btn("+", "add", "", 'aria-label="Add a computer"')}<i class="mark"></i></div></nav></div></section>`;
      if (!phone) {
        $(".act").style.top = infoTop + $(".st-info").offsetHeight + 16 + "px";
        $(".act").style.position = "absolute";
        $(".act").style.left = "24px";
        $(".act").style.right = "24px";
      }
      if (phone) {
        $(".st-tabs").style.bottom =
          `calc(${Math.max(168, $(".act").offsetHeight) + 48}px + var(--safe-bottom))`;
      }
      const tab = $(".st-tabs .on"),
        markEl = $(".mark");
      if (tab) {
        markEl.style.width = tab.offsetWidth - 22 + "px";
        markEl.style.transform = `translateX(${tab.offsetLeft + 11}px)`;
        tab.scrollIntoView({ block: "nearest", inline: "nearest" });
      }
    },
  },
  wall: {
    // Equal-height justified rows, adapted from directions.html V.wall.layout.
    layout() {
      const { W, H, TOP } = dimensions();
      const order = [...C].sort(
        (a, b) =>
          (ready(a) ? 0 : a.state.startsWith("busy") ? 1 : 2) -
          (ready(b) ? 0 : b.state.startsWith("busy") ? 1 : 2),
      );
      order.push({ id: "new", w: 16, h: 9, state: "add" });
      const n = order.length,
        pad = phone ? 16 : 30,
        gap = phone ? 12 : 22,
        label = 56,
        rowGap = phone ? 16 : 18,
        availW = W - pad * 2;
      let rows, hs;
      if (phone) {
        rows = [];
        let pair = null;
        order.forEach((c) => {
          if (ready(c)) {
            rows.push([c]);
            pair = null;
          } else if (pair && pair.length < 2) pair.push(c);
          else rows.push((pair = [c]));
        });
        hs = rows.map((row) => (availW - gap * (row.length - 1)) / row.reduce((s, c) => s + c.w / c.h, 0));
      } else if (n > 6) {
        let best = null;
        for (let count = 2; count <= Math.min(6, n); count++) {
          const rs = [];
          let start = 0;
          for (let r = 0; r < count; r++) {
            const len = Math.floor(n / count) + (r < n % count ? 1 : 0);
            rs.push(order.slice(start, start + len));
            start += len;
          }
          const heights = rs.map(
            (row) => (availW - gap * (row.length - 1)) / row.reduce((s, c) => s + c.w / c.h, 0),
          );
          const used = heights.reduce((s, h) => s + h, 0) + count * label + (count - 1) * rowGap;
          const fits = used <= H - TOP - 30;
          const area = rs.reduce(
            (s, row, r) => s + heights[r] ** 2 * row.reduce((t, c) => t + c.w / c.h, 0),
            0,
          );
          const score = area * (fits ? 1 : 0.12) * Math.min(1, Math.min(...heights) / 70);
          if (!best || score > best.score) best = { score, rows: rs, hs: heights };
        }
        rows = best.rows;
        hs = best.hs;
      } else {
        let best = null;
        for (let mask = 0; mask < 1 << (n - 1); mask++) {
          const rs = [[]];
          order.forEach((c, i) => {
            rs.at(-1).push(c);
            if ((mask >> i) & 1) rs.push([]);
          });
          if (rs.length > Math.max(3, Math.ceil(n / 4)) || rs.some((row) => row.length > 4)) continue;
          if (
            n > 6 &&
            (rs.length !== Math.ceil(n / 4) ||
              rs.some((row) => row.length < Math.floor(n / Math.ceil(n / 4))))
          )
            continue;
          const chrome = rs.length * label + (rs.length - 1) * rowGap;
          let heights = rs.map(
            (row) => (availW - gap * (row.length - 1)) / row.reduce((s, c) => s + c.w / c.h, 0),
          );
          const k = Math.min(1, (H - TOP - 35 - chrome) / heights.reduce((s, h) => s + h, 0));
          heights = heights.map((h) => Math.min(h, Math.max(88, h * Math.max(0.3, k))));
          let score = rs.reduce(
            (s, row, r) =>
              s + row.reduce((t, c) => t + ((heights[r] ** 2 * c.w) / c.h) * (ready(c) ? 1.6 : 1), 0),
            0,
          );
          if (!best || score > best.score) best = { score, rows: rs, hs: heights };
        }
        rows = best.rows;
        hs = best.hs;
      }
      const pos = {};
      let total = hs.reduce((s, h) => s + h, 0) + rows.length * (label + rowGap);
      let y = phone ? 10 : Math.max(16, (H - TOP - total) / 2);
      rows.forEach((row, r) => {
        let x = pad;
        row.forEach((c) => {
          let w = (hs[r] * c.w) / c.h;
          pos[c.id] = { x, y, w };
          x += w + gap;
        });
        y += hs[r] + label + rowGap;
      });
      pos.height = y + 25;
      return pos;
    },
    render() {
      content.innerHTML =
        toolbar() +
        `<section class="view"><div class="wall"><div class="wall-in">${C.map((c, i) => `<div class="wt ${i === sel ? "selected" : ""}" role="button" tabindex="0" data-i="${i}" aria-label="${esc(c.name)}. ${esc(statusText(c))}">${shot(c)}<div class="wl"><b>${esc(c.name)}</b>${stat(c)}</div></div>`).join("")}<div class="wt" data-i="${C.length}" role="button" tabindex="0" aria-label="Add a computer">${emptyShot()}<div class="wl"><b>Add a computer</b><small>By address and key</small></div></div></div></section>`;
      const p = this.layout();
      $(".wall-in").style.height = p.height + "px";
      $$(".wt").forEach((el) => {
        const c = C[+el.dataset.i] || { id: "new" },
          r = p[c.id];
        Object.assign(el.style, { left: r.x + "px", top: r.y + "px", width: r.w + "px" });
      });
    },
  },
};
function select(i) {
  if (i < 0 || i > C.length) return;
  if (i === C.length) return openForm();
  const oldSel = sel;
  sel = i;
  ui.fail = ui.ended = "";
  ui.remove = false;
  if (cur === "overview") {
    render();
    return;
  }
  const track = $(".st-track"),
    old = track ? getComputedStyle(track).transform : null;
  V.stage.render();
  if (old) {
    const next = $(".st-track");
    animate(next, [{ transform: old }, { transform: next.style.transform }], "move");
  }
  rememberSelection();
  updateTest();
}
function rememberSelection() {
  try {
    localStorage.setItem("sunna-proto-last", String(sel));
  } catch {}
}
function detail(s, address = "100.101.102.103") {
  return (
    {
      idle: "When a computer starts sharing, it shows its address and key.",
      checking: `Looking for Sunna at ${address}…`,
      ready: `Found ${form.name || "workstation"} · Arch Linux · 12 ms`,
      "ready-no-latency": "Found workstation · Arch Linux",
      busy: "Found workstation. Priya is connected to it right now.",
      "busy-anonymous": "Found workstation. Someone is connected to it right now.",
      key: "Sunna is there, but the key doesn't match.",
      update: "That computer runs an older Sunna. Update it.",
      "update-client": "That computer runs a newer Sunna. Update this one.",
      unreachable: `Nothing answered at ${address}. Is Sunna sharing on that computer?`,
      "not-found": `Couldn't find a computer called “${address}”.`,
      invalid: "That doesn't look like an address.",
      "invalid-ipv6": "That IPv6 address is missing its closing ']'.",
      "invalid-tail": "Unexpected text after the address.",
      "invalid-port": "The port must be a number from 1 to 65535.",
      "invalid-key": "The key can't contain line breaks.",
      "save-error": phone ? "Couldn't save it: the phone's storage may be full." : "io: permission denied",
      pending: "Saving…",
      empty: "Enter the computer's address.",
    }[s] || s
  );
}
function openForm(edit = false) {
  form = {
    address: edit ? current().address : "",
    key: edit ? current().key : phone ? "" : cfg.ownKey,
    name: edit ? current().name : "",
    state: edit ? current().state : "idle",
    revealed: false,
    link: false,
  };
  ui.scan = false;
  go(edit ? "edit" : "add");
}
function renderForm() {
  const edit = cur === "edit",
    answered = ["ready", "busy", "busy-anonymous", "key", "update", "update-client"].includes(form.state);
  const c = { ...current(), name: form.name || "workstation", state: "never", os: "Arch Linux", id: "new" };
  content.innerHTML =
    toolbar(true) +
    `<section class="view form-view">${C.length ? "" : `<p class="first-run">${phone ? "Add a computer that's sharing with Sunna: scan its QR code with this phone's camera, or paste its link here. Both appear when sharing starts; Settings in Sunna on that computer (or sunna-host link on Linux) shows them again." : "Add a computer that's sharing with Sunna. Setting up sharing prints a link (on Linux, sunna-host link shows it again); paste it here."}</p>`}<div class="form-screen">${answered ? shot(c) : emptyShot()}</div><form class="form-column" id="computer-form"><h1>${edit ? (phone ? `Edit “${esc(current().name)}”` : esc(current().name)) : "Add a computer"}</h1><label for="address">Address</label><input id="address" name="address" spellcheck="false" autocomplete="off" placeholder="100.101.102.103, a name, or a sunna:// link" value="${esc(form.address)}"><label for="key">Key</label><div class="field"><input id="key" name="key" type="${form.revealed ? "text" : "password"}" autocomplete="off" placeholder="The key it shows when it starts sharing" value="${esc(form.key)}">${btn(form.revealed ? "Hide" : "Show", "reveal", "", 'type="button" aria-label="Show key"')}</div><label for="name">Name</label><input id="name" name="name" value="${esc(form.name)}" placeholder="${answered ? "workstation (from the computer)" : phone ? "Name (optional)" : "Optional"}"><div class="check-line" role="status">${esc(detail(form.state, form.address))}${form.link ? "<br>The key came with the link." : ""}</div><div class="form-actions"><button class="primary" ${!["ready", "ready-no-latency"].includes(form.state) ? "disabled" : ""}>${edit ? "Save" : "Add computer"}</button>${btn("Cancel", "home", "", 'type="button"')}</div>${edit ? btn("Remove", "remove", "danger", 'type="button"') : `<div class="alternatives">${phone ? btn("Scan QR code", "scan", "", 'type="button"') + btn("Paste", "paste", "", 'type="button"') : btn("Find on Tailscale", "find", "", 'type="button"')}</div>`}${ui.scan ? '<p class="help">Scan it with your phone’s camera, then open the sunna:// link in Sunna.</p>' : ""}</form></section>`;
}
function checkForm() {
  clearTimeout(checkTimer);
  const link = /^sunna:\/\//i.test(form.address);
  if (link) {
    try {
      const u = new URL(form.address);
      form.address = u.hostname + (u.port ? ":" + u.port : "");
      form.key = u.searchParams.get("key") || "";
      form.link = true;
      $("#address").value = form.address;
      $("#key").value = form.key;
    } catch {}
  }
  form.state = form.address ? "checking" : "idle";
  updateFormCheck();
  checkTimer = setTimeout(() => {
    let a = form.address,
      k = form.key;
    form.state = !a
      ? "idle"
      : /[\r\n]/.test(k)
        ? "invalid-key"
        : /\s/.test(a)
          ? "invalid"
          : a.startsWith("[") && !a.includes("]")
            ? "invalid-ipv6"
            : /:(0|\d{6,})$/.test(a)
              ? "invalid-port"
              : a.startsWith("bad")
                ? "unreachable"
                : a.startsWith("missing")
                  ? "not-found"
                  : k.startsWith("wrong")
                    ? "key"
                    : a.startsWith("old")
                      ? "update"
                      : a.startsWith("newer")
                        ? "update-client"
                        : a.startsWith("busy")
                          ? "busy"
                          : "ready";
    updateFormCheck();
  }, 600);
}
function updateFormCheck() {
  if (!["add", "edit"].includes(cur)) return;
  $(".check-line").textContent =
    detail(form.state, form.address) + (form.link ? " The key came with the link." : "");
  $("#computer-form .primary").disabled = !["ready", "ready-no-latency"].includes(form.state);
  const answered = ["ready", "key", "busy", "update", "update-client"].includes(form.state),
    el = $(".form-screen");
  if (answered && !el.dataset.answered) {
    el.dataset.answered = "1";
    el.innerHTML = shot({ ...BASE[0], id: "new", name: form.name || "workstation", state: "never" });
    animate(
      el,
      [
        { transform: "scale(.97)", opacity: 0.3 },
        { transform: "none", opacity: 1 },
      ],
      "move",
    );
  }
  if (!answered) el.dataset.answered = "";
}
function saveForm() {
  if (!["ready", "ready-no-latency"].includes(form.state)) return;
  if (ui.saveError) {
    form.state = "save-error";
    updateFormCheck();
    return;
  }
  const edit = cur === "edit";
  form.state = "pending";
  updateFormCheck();
  setTimeout(
    () => {
      const c = {
        ...BASE[0],
        id: edit ? current().id : "saved-" + Date.now(),
        name: form.name || "workstation",
        address: form.address,
        key: form.key,
        state: "ready",
        never: true,
      };
      if (edit) C[sel] = c;
      else {
        C.push(c);
        sel = C.length - 1;
      }
      ui.fail = ui.ended = "";
      go("computer");
      rememberSelection();
    },
    ui.still ? 0 : 300,
  );
}
function toggle(label, key, help = "") {
  return `<div class="setting-row"><div class="words">${label}${help ? `<span class="help">${help}</span>` : ""}</div><button class="toggle" role="switch" aria-label="${label}" aria-checked="${cfg[key]}" data-toggle="${key}" ${key === "capture" && ui.permission !== "allowed" ? "disabled" : ""}></button></div>`;
}
function choice(label, key, values, help = "") {
  return `<div class="setting-row"><div class="words">${label}${help ? `<span class="help">${help}</span>` : ""}</div><div class="choices" role="radiogroup" aria-label="${label}">${values.map((x) => `<button role="radio" aria-checked="${cfg[key] === x}" data-choice="${key}" data-value="${esc(x)}">${esc(x)}</button>`).join("")}</div></div>`;
}
function selectChoice(label, key, values) {
  return `<div class="choice-list" role="radiogroup" aria-label="${label}"><h3>${label}</h3>${values.map((x) => `<button class="item choice-option" role="radio" aria-checked="${cfg[key] === x}" data-choice="${key}" data-value="${esc(x)}" ${ui.phase === "waiting" ? "disabled" : ""}><span>${esc(x === "Native" ? `Native (${current().w}×${current().h})` : x)}</span><span aria-hidden="true">${cfg[key] === x ? "✓" : ""}</span></button>`).join("")}</div>`;
}
function videoRows() {
  return phone
    ? choice("Sharpness", "resolution", ["Full", "75%", "50%"]) +
        choice("Frame rate", "fps", ["60", "30"]) +
        choice("Bitrate", "bitrate", ["Auto", "10", "20", "40", "80"]) +
        choice("Codec", "codec", ["Auto", "HEVC", "H.264"])
    : selectChoice("Codec", "codec", ["HEVC (sharper per bit)", "H.264 (most compatible)"]) +
        selectChoice("Resolution", "resolution", [
          "Native",
          "75% (less to send, softer)",
          "50% (less to send, softer)",
        ]) +
        selectChoice("Frame rate", "fps", ["60 fps", "30 fps (half the work for slow computers)"]) +
        selectChoice("Bitrate limit", "bitrate", ["10 Mbps", "20 Mbps", "40 Mbps", "80 Mbps"]) +
        toggle("Fast lane (lossless text while typing)", "fast");
}
function permissionRow() {
  if (phone)
    return `<div class="setting-row"><div class="words">Notifications<span class="help">While you're connected to a computer</span></div>${ui.permission === "allowed" ? "<small>Allowed</small>" : btn("Allow…", "allow", "secondary")}${ui.permission === "denied" ? "<small>Notifications denied</small>" : ""}</div>`;
  return `<div class="setting-row"><div class="words">System shortcuts<span class="help">⌘Tab, ⌘Space and the like go to the other computer. macOS asks you to allow this once, under Accessibility.</span></div>${ui.permission === "allowed" ? "<small>Allowed</small>" : btn("Allow…", "allow", "secondary")}</div>${["reset", "opened", "reset-failed"].includes(ui.permission) ? `<div class="setting-row"><div class="words"><span class="help">${ui.permission === "reset-failed" ? "Couldn't reset it. In the list that opened, select Sunna, remove it with −, then allow Sunna again." : "Sunna is already on in that list, but this still says no? That entry is from an earlier build: reset it, then allow Sunna again."}</span></div>${btn("Reset", "reset-permission", "secondary")}</div>` : ""}`;
}
function selfStatus() {
  return (
    {
      loading: "Checking…",
      ready: "Sharing at 100.101.102.10",
      busy: "Sharing at 100.101.102.10 · in a session",
      key: "Sharing, but with a different key than the one below",
      network: "Not on a Tailscale network",
      linux: "Not sharing. Run sunna-host setup to share it.",
      mac: "Not sharing. To share this Mac, run scripts/run.sh host.",
    }[ui.self] || ui.self
  );
}
function renderSettings() {
  content.innerHTML =
    toolbar(true) +
    `<section class="view settings-view"><div class="settings-column"><h1>Settings</h1><h3>${phone ? "This phone" : "This computer"}</h3>${phone ? `<div class="setting-row"><div class="words">${mark({ os: "Android" })} This phone<span class="help">What computers call this phone while it's connected: “Pixel 9 is connected to it.”</span></div><input aria-label="This phone" data-own="ownName" value="${esc(cfg.ownName)}"></div>` : `<div class="setting-row"><div class="words">${mark({ os: "macOS" })} Sunny’s Mac<span class="help">${selfStatus()}</span></div></div><div class="setting-row"><div class="words">This computer's key<span class="help">Other computers need it to connect to this one. New computers you add start with it.</span></div></div><div class="field"><input data-own="ownKey" aria-label="This computer's key" type="${ui.keyRevealed ? "text" : "password"}" value="${esc(cfg.ownKey)}">${btn(ui.keyRevealed ? "Hide" : "Show", "reveal-self")}${btn("Copy", "copy-key")}</div><div class="setting-row"><div class="words">Link to this computer<span class="help mono">sunna://100.101.102.10?key=••••</span></div><div class="row">${btn("Copy", "copy-self")}${btn("Show QR code", "qr-self")}</div></div>`}<h3>Sessions</h3>${phone ? choice("Touch", "touch", ["Trackpad", "Touch"], "Trackpad: drag to move the pointer, tap to click. Touch: tap where you want to click.") + toggle("Sound", "sound", "Play the computer's sound on this phone") + toggle("Share the clipboard", "clipboard", "Copy on one, paste on the other, both ways") + toggle("Keyboard button", "keyboardButton", "Next to ••• at the top of sessions, for the keyboard in one tap") + toggle("Keyboard suggestions", "suggestions", "Corrections and swipe typing from your keyboard; Sunna retypes what changes") + toggle("Stats bar", "stats", "Latency, frame rate and bitrate during sessions") + (ui.permissionShown ? permissionRow() : "") : permissionRow() + choice("Open sessions", "openSessions", ["Full screen", "Window"], "⌃⌥F switches during a session.") + toggle("Stats bar", "stats", "Latency, bitrate and codec along the top. ⌃⌥S toggles it.") + toggle("Menu button", "menu", "The ••• in the corner. ⌃⌥M opens the menu either way.") + toggle("Sound", "sound", "Play the other computer's sound. Also in the session menu.")}<h3>Screens</h3>${toggle("Remember each computer's last screen", "remember") + toggle("Live previews", "previews")}${
      phone
        ? '<h3>Video</h3><p class="help">How sessions start; the session menu changes them for the session you’re in.</p>' +
          videoRows()
        : `<h3>Shortcuts</h3>${[
            ["Session menu", "⌃⌥M"],
            ["Give ⌘Tab and other shortcuts back to this Mac", "⌃⌥G"],
            ["Full screen or window", "⌃⌥F"],
            ["Stats bar", "⌃⌥S"],
            ["Disconnect", "⌃⌥Q"],
          ]
            .map(([l, k]) => `<div class="setting-row"><span>${l}</span><kbd>${k}</kbd></div>`)
            .join("")}`
    }<h3>About</h3><div class="setting-row"><span>Sunna 0.0.1</span><small>protocol 8</small></div>${phone ? `<div class="setting-row"><div class="words">Open source software<span class="help">The software Sunna is made with, and its licenses</span></div>${btn("›", "licenses", "icon", 'aria-label="Open source software"')}</div>` : ""}<p class="credits">The Linux penguin is drawn after Tux by Larry Ewing (lewing@isc.tamu.edu), made with The GIMP.</p></div></section>`;
}
function renderLicenses() {
  content.innerHTML =
    toolbar(true) +
    `<section class="view settings-view"><div class="settings-column"><h1>Open source software</h1>${btn("Close", "settings")}<div class="mono help" style="margin-top:20px;white-space:pre-wrap">${ui.licensesEmpty ? "" : "Prototype list\n\nRust — MIT / Apache-2.0\n\nKotlin — Apache-2.0\n\nFFmpeg — LGPL\n\nThe release app includes full dependency license notices."}</div></div></section>`;
}
function renderFind() {
  let states = ["Sharing", "In use", "Other key", "Update", "Added", "Not sharing", "Offline"];
  content.innerHTML =
    toolbar(true) +
    `<section class="view find-view"><div class="settings-column"><div class="row">${btn("‹ Back", "form-back")}<h1>On your tailnet</h1></div><p>Computers on your Tailscale network that are sharing with Sunna. Choose one to fill in its address.</p>${btn("Look again", "rescan", "secondary")}<div style="margin-top:18px">${["loading", "rescan"].includes(ui.discovery) ? "<p>Looking on your tailnet…</p>" : ui.discovery === "empty" ? "<p>No computers on your tailnet are sharing with Sunna yet.</p>" : ui.discovery === "error" ? "<p>Tailscale isn't installed on this computer.</p>" : ui.discovery === "signed-out" ? "<p>Tailscale isn't connected. Open Tailscale and sign in.</p>" : ui.discovery === "output" ? "<p>Unexpected Tailscale output: invalid JSON</p>" : states.map((s, i) => `${i === 5 ? "<h3>Not sharing · 2</h3>" : ""}<button class="device-row" data-discovery="${i}" ${i >= 4 ? "disabled" : ""}><span>${esc(["workstation", "studio", "ubuntu", "old-laptop", "archlinux", "printer", "travel-laptop"][i])}<small>${i === 6 ? "macOS" : "Linux"} · 100.101.102.${120 + i}</small></span><span>${s}</span></button>`).join("")}</div></div></section>`;
}
function qrGrid() {
  let s = "";
  for (let y = 0; y < 29; y++)
    for (let x = 0; x < 29; x++) {
      let on = false;
      for (const [ox, oy] of [
        [1, 1],
        [21, 1],
        [1, 21],
      ]) {
        let a = x - ox,
          b = y - oy;
        if (a >= 0 && a < 7 && b >= 0 && b < 7)
          on = a === 0 || a === 6 || b === 0 || b === 6 || (a >= 2 && a <= 4 && b >= 2 && b <= 4);
      }
      const finder = (x < 9 && y < 9) || (x > 19 && y < 9) || (x < 9 && y > 19);
      if (!finder) on = (x * 17 + y * 31 + x * y * 3) % 11 < 5;
      s += `<i class="${on ? "" : "blank"}"></i>`;
    }
  return s;
}
function renderQR() {
  const c = ui.qrSelf ? { name: "Sunny’s Mac", address: "100.101.102.10", key: cfg.ownKey } : current();
  content.innerHTML =
    toolbar(true) +
    `<section class="view qr-view"><h1>Add “${esc(c.name)}” on a phone</h1><div class="qr-wrap">${ui.qr === "error" ? '<div class="shot empty-screen" style="aspect-ratio:1">Couldn\'t make the QR code.</div>' : ui.qr === "generating" ? '<div class="shot empty-screen" style="aspect-ratio:1">Looking…</div>' : `<div class="qr-grid" role="img" aria-label="Computer link QR code">${qrGrid()}</div>`}</div><div class="qr-link">sunna://${esc(c.address)}?key=${esc(c.key)}</div>${btn("Copy link", ui.qrSelf ? "copy-self" : "copy-link", "secondary")}<p>Scan it with your phone's camera to add this computer to Sunna on the phone.</p><p class="help">It contains the computer's key: show it only to your own devices.</p>${btn("Done", ui.qrSelf ? "settings" : "home")}</section>`;
}
function statsText() {
  if (ui.phase === "waiting" || cur === "connecting") return ["connecting…"];
  const c = current(),
    level =
      { basic: 0, host: 1, latency: 2, decoder: 3, rtt: 4, loss: 5, sound: 6, full: 7 }[ui.statsDetail] ?? 7;
  let a = [
    `${cfg.codec.startsWith("H.264") ? "H264" : "HEVC"} ${c.w}×${c.h}`,
    "60 fps",
    level ? "8.4 / 20 Mbps" : "8.4 Mbps",
  ];
  if (level >= 2) a.push("latency 12 ms (p95 18)");
  if (level >= 1) a.push("encode 2.1 ms");
  if (level >= 3) a.push("decode 1.4 ms");
  if (level >= 4) a.push("rtt 7 ms");
  if (level >= 5) a.push("lost 2");
  if (level >= 6 && cfg.sound) a.push("sound 38 ms");
  if (level >= 7) a.push("tiles 4/s");
  a.push(phone ? ui.decoder + " decoder" : "⌃⌥S hide");
  return a;
}
function capsule() {
  if (cur === "connecting") {
    let text =
      {
        reaching: `Reaching ${current().name}…`,
        video: "Starting video…",
        connected: "Connected · 12 ms",
        ended: "Disconnected",
        cancel: "Disconnected",
      }[ui.phase] || "Starting video…";
    return `<div class="capsule"><i class="spinner"></i><span>${esc(text)}</span>${phone ? "" : "<small>esc to cancel</small>"}</div>`;
  }
  if (ui.notice) return `<div class="capsule">${esc(NOTICES[ui.notice] || ui.notice)}</div>`;
  return cfg.stats
    ? `<div class="capsule stats">${statsText()
        .map((t) => `<span>${esc(t)}</span>`)
        .join("<span>·</span>")}</div>`
    : "";
}
function controlIcon(action) {
  const path = action.includes("keyboard")
    ? '<rect x="2" y="5" width="20" height="14" rx="2"/><path d="M5 9h1m3 0h1m3 0h1m3 0h1M5 12h1m3 0h1m3 0h1m3 0h1M7 16h10"/>'
    : action === "disconnect"
      ? '<path d="M12 2v9M7 5a8 8 0 1 0 10 0"/>'
      : action === "sound"
        ? '<path d="M4 9h4l5-4v14l-5-4H4zM17 8q5 4 0 8"/>'
        : action === "fullscreen"
          ? '<rect x="3" y="4" width="18" height="16" rx="1"/><path d="M3 8h18"/>'
          : action === "stats"
            ? '<path d="M4 20V12M9 20V7M14 20V3M19 20V9"/>'
            : action.includes("clipboard")
              ? '<path d="M9 4H5v17h14V4h-4"/><rect x="9" y="2" width="6" height="5" rx="1"/>'
              : action.includes("video")
                ? '<rect x="2" y="4" width="20" height="14" rx="2"/><path d="M8 22h8M12 18v4"/>'
                : action === "touch"
                  ? '<path d="m5 3 2 16 4-5 6 1z"/>'
                  : '<circle cx="5" cy="12" r="1"/><circle cx="12" cy="12" r="1"/><circle cx="19" cy="12" r="1"/>';
  return `<svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round">${path}</svg>`;
}
function control(symbol, label, action, state = "", danger = false) {
  return btn(
    `<span class="control-icon" aria-hidden="true">${controlIcon(action)}</span><span>${label}</span>${state ? `<span class="state">${state}</span>` : ""}`,
    action,
    "menu-control" + (danger ? " danger" : ""),
  );
}
function shortcuts() {
  return /mac/i.test(current().os) ? MAC_KEYS : LINUX_KEYS;
}
function sendKeys() {
  return shortcuts()
    .map((k) => btn(esc(k), "send-key", "item", `data-key="${esc(k)}"`))
    .join("");
}
function otherComputers() {
  return `<p class="other-title">Other computers</p><div class="other-strip">${C.map((c, i) => (i === sel ? "" : `<button class="other" style="width:${Math.min(230, (96 * c.w) / c.h)}px" data-switch="${i}" aria-label="Switch to ${esc(c.name)}">${shot(c)}<span>${esc(c.name)}</span></button>`)).join("")}</div>`;
}
function menuControls() {
  return phone
    ? `<div class="menu-heading"><b>${esc(current().name)}</b>${btn("Close", "close-menu")}</div><p class="help">HEVC · ${current().w}×${current().h} · 60 fps</p><div class="menu-controls">${control("⌨", "Keyboard", "keyboard")}${control("↖", cfg.touch, "touch", cfg.touch)}${control("♪", cfg.sound ? "Sound on" : "Sound off", "sound")}${control("▥", "Stats", "stats", cfg.stats ? "On" : "Off")}</div><div class="menu-phone-group"><h3>Shortcuts</h3><div class="shortcut-list">${sendKeys()}</div><h3>Keyboard</h3>${toggle("Keyboard button", "keyboardButton", "Next to ••• at the top, for the keyboard in one tap")}${toggle("Keyboard suggestions", "suggestions", "Corrections and swipe typing; Sunna retypes what changes")}<h3>Clipboard</h3>${toggle("Share the clipboard", "clipboard")}${btn("Type clipboard text", "type-clipboard", "item")}<p class="help">Key by key, for login screens and password prompts</p><h3>Video</h3>${videoRows()}</div>${control("⏻", "Disconnect", "disconnect", "", true)}${otherComputers()}`
    : `<div class="menu-heading"><b>${esc(current().name)}</b><span>${ui.phase === "waiting" ? "Connecting…" : `HEVC ${current().w}×${current().h} · 60 fps · 12 ms`}</span></div><div class="menu-controls">${control("⛶", cfg.fullscreen ? "Windowed" : "Full screen", "fullscreen")}${control("▥", "Stats bar", "stats", cfg.stats ? "On" : "Off")}${control("♪", "Play sound", "sound", cfg.sound ? "Sound on" : "Sound off")}${control("⌨", "Keyboard", "submenu-keyboard")}${control("⇄", "Clipboard", "submenu-clipboard")}${control("▣", "Video", "submenu-video")}${control("•••", cfg.menu ? "Hide menu button" : "Show menu button", "menu-visible")}${control("⏻", "Disconnect", "disconnect", "", true)}</div>${otherComputers()}`;
}
function submenu() {
  return ui.submenu
    ? `<div class="submenu" data-submenu="${ui.submenu}">${btn("×", "close-submenu", "close-submenu", 'aria-label="Close submenu"')}<h2>${ui.submenu[0].toUpperCase() + ui.submenu.slice(1)}</h2>${ui.submenu === "keyboard" ? toggle("Send ⌘ shortcuts to remote", "capture") + "<h3>Send keys</h3>" + sendKeys() : ui.submenu === "clipboard" ? toggle("Share clipboard", "clipboard") + btn("Type clipboard text", "type-clipboard", "item") : videoRows()}</div>`
    : "";
}
function keyboard() {
  const mods = /mac/i.test(current().os) ? ["⌃", "⌥", "⌘", "⇧"] : ["Ctrl", "Alt", "Super", "Shift"];
  let keys = [
    ...mods,
    "esc",
    "tab",
    "←",
    "↑",
    "↓",
    "→",
    /mac/i.test(current().os) ? "⌦" : "Del",
    "home",
    "end",
    "pg↑",
    "pg↓",
    ...Array.from({ length: 12 }, (_, i) => "F" + (i + 1)),
  ];
  return `<div class="keyboard-panel"><div class="accessory">${btn("⌄", "keyboard", "", 'aria-label="Hide keyboard"')}${keys.map((k) => `<button class="${ui.modifier[k] || ""}" data-keyboard-key="${k}" ${mods.includes(k) ? 'data-modifier="1"' : ""}>${k}</button>`).join("")}</div>${cfg.suggestions ? '<div class="suggestions"><span>the</span><span>hello</span><span>and</span></div>' : ""}<div class="keyboard-keys">${["qwertyuiop", "asdfghjkl", "zxcvbnm"].map((row, i) => `<div class="keyrow">${i === 2 ? '<button data-keyboard-key="Shift" aria-label="Shift">⇧</button>' : ""}${[...row].map((k) => `<button data-keyboard-key="${k}">${k}</button>`).join("")}${i === 2 ? '<button data-keyboard-key="Backspace" aria-label="Backspace">⌫</button>' : ""}</div>`).join("")}<div class="keyrow">${btn("123", "keyboard-key")}${btn("space", "keyboard-key", "space")}${btn("↵", "keyboard-key")}</div></div></div>`;
}
function wakeButtons() {
  clearTimeout(idleTimer);
  const f = $(".float-buttons");
  if (f) {
    f.style.opacity = "1";
    if (!ui.still)
      idleTimer = setTimeout(() => {
        const el = $(".float-buttons");
        if (el) el.style.opacity = ".55";
      }, 3000);
  }
}
function renderSession() {
  const W = app.clientWidth,
    H = app.clientHeight,
    c = current(),
    menu = cur === "menu",
    landscape = phone && H < 500;
  let w, h, x, y;
  const availH = ui.keyboard ? H - (cfg.suggestions ? 368 : 330) : H;
  if (menu) {
    if (phone) {
      w = landscape ? W * 0.43 : W * 0.82;
      h = (w * c.h) / c.w;
      x = landscape ? W * 0.035 : (W - w) / 2;
      y = landscape ? (H - h) / 2 : 28;
    } else {
      w = Math.min(W * 0.76, ((H - 350) * c.w) / c.h);
      h = (w * c.h) / c.w;
      x = (W - w) / 2;
      y = 24;
    }
  } else {
    w = Math.min(W, (availH * c.w) / c.h);
    h = (w * c.h) / c.w;
    x = (W - w) / 2;
    y = (availH - h) / 2;
  }
  content.innerHTML = `<section class="session ${cur}" aria-label="Sunna — ${esc(c.name)}"><div class="sess-shot" style="left:${x}px;top:${y}px;width:${w}px;${ui.zoom > 1 ? `transform:scale(${ui.zoom});transform-origin:center;` : ""}">${shot(c, true)}${cur === "connecting" ? '<div class="connection-shade"></div>' : ""}${ui.pointer === "outside" || menu ? "" : '<i class="pointer" aria-hidden="true"></i>'}</div>${menu ? (ui.notice ? capsule() : "") : capsule()}${cur === "connecting" ? btn("Cancel", "cancel", "cancel-connect") : menu ? `<div class="session-controls" style="top:${y + h + 16}px">${menuControls()}</div>${phone ? "" : submenu()}` : phone ? `<div class="float-buttons" style="${ui.floatX !== undefined ? `left:${ui.floatX}px;` : ""}" aria-label="Movable session buttons">${cfg.keyboardButton ? btn(controlIcon("keyboard"), "keyboard", "", 'aria-label="Keyboard"') : ""}${btn("•••", "menu", "", 'aria-label="Session menu"')}</div>` : cfg.menu ? btn("•••", "menu", "session-more", 'aria-label="Session menu"') : ""}${phone && ui.keyboard ? keyboard() : ""}</section>`;
  if (phone && !menu) {
    wakeButtons();
    positionButtons();
  }
  if (menu && phone && ui.submenu) {
    const headings = $$(".session-controls h3");
    const h = headings.find((x) => x.textContent.toLowerCase() === ui.submenu);
    if (h) $(".session-controls").scrollTop = h.offsetTop - 15;
  }
}
function connect() {
  if (!C.length) return openForm();
  if (!ready(current()) && !ui.fail && !ui.ended) {
    if (current().state === "key") $("#key-fix input")?.focus();
    return;
  }
  clearTimeout(timer);
  ui.fail = ui.ended = "";
  ui.notice = "";
  ui.keyboard = false;
  ui.phase = "reaching";
  ui.submenu = "";
  if (!phone) cfg.fullscreen = cfg.openSessions !== "Window";
  sessionEpoch++;
  const epoch = sessionEpoch;
  rememberSelection();
  go("connecting");
  timer = setTimeout(() => {
    if (epoch !== sessionEpoch) return;
    ui.phase = "video";
    updateCapsule();
    timer = setTimeout(() => {
      if (epoch !== sessionEpoch) return;
      if (ui.nextFail !== "success") {
        leave(ui.nextFail, false);
        return;
      }
      cur = "session";
      ui.phase = "connected";
      renderSession();
      if (!ui.still) {
        const shade = document.createElement("div");
        shade.className = "connection-shade";
        $(".sess-shot .shot").append(shade);
        shade
          .animate([{ opacity: 0.45 }, { opacity: 0 }], { duration: reduced() ? 150 : 300, easing: "linear" })
          .finished.then(() => shade.remove());
      }
      ui.notice = "Connected · 12 ms";
      updateCapsule();
      timer = setTimeout(() => {
        if (epoch === sessionEpoch) {
          ui.notice = "";
          updateCapsule();
        }
      }, 1600);
    }, 850);
  }, 650);
}
function leave(reason = "", ended = true) {
  clearTimeout(timer);
  sessionEpoch++;
  ui.keyboard = false;
  ui.notice = "";
  ui.submenu = "";
  ui.zoom = 1;
  if (reason) {
    ui[ended ? "ended" : "fail"] = reason;
  } else {
    ui.fail = ui.ended = "";
  }
  go("computer");
}
function positionButtons() {
  const f = $(".float-buttons"),
    cap = $(".session>.capsule");
  if (!f) return;
  f.style.top =
    Math.max(app.clientHeight < 500 ? 12 : 62, cap ? cap.offsetTop + cap.offsetHeight + 12 : 0) + "px";
}
function updateCapsule() {
  if (!["session", "connecting", "menu"].includes(cur)) return;
  $(".session>.capsule")?.remove();
  $(".session")?.insertAdjacentHTML("beforeend", capsule());
  positionButtons();
}
function notice(name) {
  ui.notice = name;
  clearTimeout(noticeTimer);
  updateCapsule();
  if (!["session", "connecting", "menu"].includes(cur))
    toast(NOTICES[name] || name, name === "action" ? "edit" : "");
  if (cur === "menu") {
    $(".session>.capsule")?.remove();
    $(".session").insertAdjacentHTML("beforeend", capsule());
  }
  if (!ui.still)
    noticeTimer = setTimeout(
      () => {
        ui.notice = "";
        updateCapsule();
      },
      phone ? 4500 : 6000,
    );
}
function toast(text, action = "") {
  clearTimeout(toastTimer);
  $("#feedback").innerHTML =
    `<div class="capsule toast">${esc(text)}${action ? btn(action === "edit" ? "Edit" : "Allow…", action) : ""}</div>`;
  if (!ui.still)
    toastTimer = setTimeout(() => {
      $("#feedback").innerHTML = "";
    }, 4500);
}
async function copy(text, label) {
  let ok = false;
  try {
    await navigator.clipboard.writeText(text);
    ok = true;
  } catch {
    const t = document.createElement("textarea");
    t.value = text;
    t.style.cssText = "position:absolute;left:-10000px";
    app.append(t);
    t.select();
    try {
      ok = document.execCommand("copy");
    } catch {}
    t.remove();
  }
  toast(
    ok ? label || (phone ? "Copied " + text : "Copied.") : "Couldn't copy. Select the text and press ⌘C.",
  );
}
async function paste() {
  try {
    const text = await navigator.clipboard.readText();
    if (!text) {
      toast("The clipboard is empty.");
      return;
    }
    form.address = text;
    $("#address").value = text;
    checkForm();
  } catch {
    toast("Paste the link into Address.");
    $("#address")?.focus();
  }
}
async function typeClipboard() {
  try {
    const text = await navigator.clipboard.readText();
    if (!text) return notice("clipboard-empty");
    const chars = [...text.slice(0, 4000)],
      typed = chars.filter((x) => x.charCodeAt(0) < 128).length,
      skipped = chars.length - typed;
    notice(
      `Typing ${typed} characters${skipped ? `${phone ? " · " : "  ·  "}${skipped} skipped (not on a US keyboard)` : ""}`,
    );
  } catch {
    notice("clipboard-empty");
  }
}
function videoChanged() {
  if (["menu", "session"].includes(cur)) {
    notice(ui.streamError ? "stream-error" : phone ? "" : "changing");
    if (!phone && !ui.streamError) setTimeout(() => notice("changed"), 600);
  }
}
function render() {
  $$("#app > .context,#app > .submenu").forEach((el) => el.remove());
  app.classList.toggle("still", ui.still);
  app.classList.toggle("menu-hover", ui.menuHover);
  app.classList.toggle("reduced", ui.reduced);
  if (!C.length && ["computer", "overview"].includes(cur)) cur = "add";
  switch (cur) {
    case "computer":
      V.stage.render();
      break;
    case "overview":
      V.wall.render();
      break;
    case "add":
    case "edit":
      renderForm();
      break;
    case "find":
      renderFind();
      break;
    case "settings":
      renderSettings();
      break;
    case "licenses":
      renderLicenses();
      break;
    case "qr":
      renderQR();
      break;
    default:
      renderSession();
  }
  if (ui.permissionShown && cur === "computer") {
    const act = $(".act");
    act.insertAdjacentHTML("beforeend", `<div class="permission-inline">${permissionRow()}</div>`);
  }
  const popup = $(".context");
  if (popup) anchorList(popup, $(".more-computer"));
  const sub = $(".submenu");
  if (sub) anchorList(sub, $(`[data-action="submenu-${ui.submenu}"]`), true);
  if (cur === "menu" && ui.submenu === "video" && ui.phase === "waiting")
    $$(".submenu select,.submenu [data-choice],.submenu [data-toggle]").forEach((el) => (el.disabled = true));
  bindGestures();
}
// Position in app coordinates, including keyboard/deep-link invocation.
function anchorList(list, control, above = false) {
  if (!control) return;
  app.append(list);
  const r = box(control),
    pad = 12,
    gap = 6;
  Object.assign(list.style, {
    position: "absolute",
    transform: "none",
    bottom: "auto",
    right: "auto",
    maxHeight: `${app.clientHeight - 2 * pad}px`,
  });
  const below = app.clientHeight - r.y - r.h - gap - pad;
  const up = above || (list.scrollHeight > below && r.y > below);
  const room = up ? r.y - gap - pad : below;
  list.style.maxHeight = `${Math.max(44, above ? Math.min(room, 360) : room)}px`;
  list.style.left = `${Math.max(pad, Math.min(app.clientWidth - list.offsetWidth - pad, r.x + r.w / 2 - list.offsetWidth / 2))}px`;
  list.style.top = `${up ? Math.max(pad, r.y - gap - list.offsetHeight) : r.y + r.h + gap}px`;
}
function updateTest() {
  if (typeof renderTest === "function") renderTest();
}
function toggleSetting(key) {
  cfg[key] = !cfg[key];
  const scroll = $(".session-controls")?.scrollTop || $(".settings-view")?.scrollTop || 0;
  render();
  const sc = $(".session-controls") || $(".settings-view");
  if (sc) sc.scrollTop = scroll;
  if (key === "capture") notice(cfg.capture ? "captured" : "released");
  if (key === "clipboard" && !phone) notice(cfg.clipboard ? "clipboard-on" : "clipboard-off");
  if (key === "fast") videoChanged();
}
function act(action, b) {
  switch (action) {
    case "overview":
      go("overview");
      break;
    case "home":
      ui.remove = false;
      ui.fail = ui.ended = "";
      clearTimeout(checkTimer);
      go(C.length ? "computer" : "add");
      break;
    case "settings":
      go("settings");
      break;
    case "add":
      openForm();
      break;
    case "edit":
      openForm(true);
      break;
    case "context":
      ui.context = !ui.context;
      render();
      break;
    case "connect":
      connect();
      break;
    case "remove":
      ui.remove = true;
      go("computer");
      break;
    case "keep":
      ui.remove = false;
      render();
      break;
    case "confirm-remove":
      if (ui.removeError) {
        toast(phone ? "Couldn't remove it: the phone's storage may be full." : "io: permission denied");
        break;
      }
      const name = current().name;
      C.splice(sel, 1);
      sel = Math.max(0, Math.min(sel, C.length - 1));
      ui.remove = false;
      if (!C.length) form = { address: "", key: "", name: "", state: "idle", revealed: false };
      go(C.length ? "computer" : "add");
      if (!phone) toast(`Removed ${name}.`);
      break;
    case "copy-address":
      copy(current().address);
      ui.context = false;
      render();
      break;
    case "copy-link":
      copy(`sunna://${current().address}?key=${current().key}`);
      ui.context = false;
      if (cur === "computer") render();
      break;
    case "copy-key":
      copy(cfg.ownKey);
      break;
    case "copy-self":
      copy("sunna://100.101.102.10?key=" + cfg.ownKey);
      break;
    case "qr":
      ui.qrSelf = false;
      go("qr");
      break;
    case "qr-self":
      ui.qrSelf = true;
      go("qr");
      break;
    case "reveal":
      form.revealed = !form.revealed;
      $("#key").type = form.revealed ? "text" : "password";
      b.textContent = form.revealed ? "Hide" : "Show";
      b.setAttribute("aria-label", form.revealed ? "Hide key" : "Show key");
      break;
    case "reveal-self":
      ui.keyRevealed = !ui.keyRevealed;
      render();
      break;
    case "find":
      ui.discovery = "loading";
      go("find");
      timer = setTimeout(() => {
        if (cur === "find") {
          ui.discovery = "results";
          render();
        }
      }, 600);
      break;
    case "rescan":
      ui.discovery = "rescan";
      render();
      timer = setTimeout(() => {
        if (cur === "find") {
          ui.discovery = "results";
          render();
        }
      }, 600);
      break;
    case "form-back":
      go("add");
      break;
    case "paste":
      paste();
      break;
    case "scan":
      ui.scan = !ui.scan;
      render();
      break;
    case "cancel":
      leave("", false);
      break;
    case "disconnect":
      leave();
      break;
    case "menu":
      ui.keyboard = false;
      ui.submenu = "";
      go("menu");
      break;
    case "close-menu":
      ui.submenu = "";
      go("session");
      break;
    case "fullscreen":
      cfg.fullscreen = !cfg.fullscreen;
      render();
      break;
    case "stats":
      toggleSetting("stats");
      break;
    case "sound":
      toggleSetting("sound");
      if (!phone) notice(cfg.sound ? "sound-on" : "sound-off");
      break;
    case "touch":
      cfg.touch = cfg.touch === "Touch" ? "Trackpad" : "Touch";
      render();
      notice(cfg.touch.toLowerCase());
      break;
    case "menu-visible":
      cfg.menu = !cfg.menu;
      render();
      if (!cfg.menu) notice("hidden");
      break;
    case "submenu-keyboard":
    case "submenu-clipboard":
    case "submenu-video":
      ui.submenu = action.slice(8);
      render();
      break;
    case "close-submenu":
      ui.submenu = "";
      render();
      break;
    case "send-key":
      notice("Sent " + b.dataset.key);
      break;
    case "type-clipboard":
      typeClipboard();
      break;
    case "keyboard":
      ui.keyboard = !ui.keyboard;
      if (cur === "menu") cur = "session";
      render();
      break;
    case "keyboard-key":
      Object.keys(ui.modifier).forEach((k) => {
        if (ui.modifier[k] === "armed") delete ui.modifier[k];
      });
      render();
      break;
    case "allow":
      ui.permission = "opened";
      render();
      toast(phone ? "Notification permission requested." : "System Settings → Accessibility opened.");
      break;
    case "reset-permission":
      ui.permission = "reset";
      render();
      toast("System Settings → Accessibility opened.");
      break;
    case "licenses":
      go("licenses");
      break;
  }
  updateTest();
}
app.addEventListener("click", (e) => {
  if (dragged) return;
  const b = e.target.closest("[data-action]");
  if (b) {
    if (b.disabled) return;
    act(b.dataset.action, b);
    return;
  }
  const sw = e.target.closest("[data-switch]");
  if (sw) {
    const from = $(".shot", sw),
      old = current(),
      oldRect = box($(".sess-shot .shot")),
      smallRect = box(from);
    sel = +sw.dataset.switch;
    if (ready(current())) {
      ui.notice = "";
      ui.phase = "connected";
      go("session", {
        from,
        onStart: () => {
          if (!ui.still && !reduced()) {
            const ghost = document.createElement("div");
            ghost.className = "switch-ghost";
            ghost.style.cssText = `position:absolute;z-index:10;pointer-events:none;left:${smallRect.x}px;top:${smallRect.y}px;width:${smallRect.w}px;transform-origin:0 0`;
            ghost.innerHTML = shot(old, true);
            app.append(ghost);
            const a = animate(ghost, [
              {
                transform: `translate(${oldRect.x - smallRect.x}px,${oldRect.y - smallRect.y}px) scale(${oldRect.w / smallRect.w})`,
                opacity: 1,
              },
              { transform: "none", opacity: 0 },
            ]);
            a?.finished.then(() => ghost.remove()).catch(() => ghost.remove());
          }
        },
      });
      rememberSelection();
    } else {
      ui.fail = ui.ended = "";
      go("computer", { from });
    }
    return;
  }
  const t = e.target.closest("[data-toggle]");
  if (t) {
    toggleSetting(t.dataset.toggle);
    return;
  }
  const ch = e.target.closest("[data-choice]");
  if (ch) {
    cfg[ch.dataset.choice] = ch.dataset.value;
    const sc = $(".session-controls") || $(".settings-view"),
      top = sc?.scrollTop;
    render();
    const sc2 = $(".session-controls") || $(".settings-view");
    if (sc2) sc2.scrollTop = top;
    videoChanged();
    return;
  }
  const d = e.target.closest("[data-discovery]");
  if (d) {
    form.address = "100.101.102." + (120 + +d.dataset.discovery);
    form.name = ["workstation", "studio", "ubuntu", "old-laptop"][+d.dataset.discovery];
    form.state = "checking";
    go("add");
    checkForm();
    $("#key").focus();
    return;
  }
  const tab = e.target.closest("[data-select]");
  if (tab) return select(+tab.dataset.select);
  const slide = e.target.closest(".st-slide");
  if (slide) {
    let i = +slide.dataset.i;
    if (i === sel) {
      if (phone) connect();
    } else select(i);
    return;
  }
  const tile = e.target.closest(".wt");
  if (tile) {
    let i = +tile.dataset.i;
    if (i === C.length) return openForm();
    sel = i;
    go("computer", { from: $(".shot", tile) });
    return;
  }
  const key = e.target.closest("[data-keyboard-key]");
  if (key) {
    let k = key.dataset.keyboardKey;
    if (key.dataset.modifier) {
      const old = ui.modifier[k],
        now = Date.now();
      ui.modifier[k] =
        old === "locked"
          ? ""
          : old === "armed" && now - (ui.lastModTime || 0) < 400
            ? "locked"
            : old === "armed"
              ? ""
              : "armed";
      ui.lastModTime = now;
    } else
      Object.keys(ui.modifier).forEach((x) => {
        if (ui.modifier[x] === "armed") delete ui.modifier[x];
      });
    render();
    return;
  }
  if (cur === "menu" && e.target.closest(".sess-shot")) act("close-menu");
  else if (ui.context && !e.target.closest(".context,.name-row")) {
    ui.context = false;
    render();
  }
});
app.addEventListener("dblclick", (e) => {
  if ((e.target.closest(".st-slide.on") || e.target.closest(".wt")) && !phone) connect();
});
app.addEventListener("contextmenu", (e) => {
  if (e.target.closest(".st-info,.st-slide,.wt")) {
    e.preventDefault();
    const t = e.target.closest("[data-i]");
    if (t) sel = Math.min(+t.dataset.i, C.length - 1);
    cur = "computer";
    ui.context = true;
    render();
  }
});
app.addEventListener("submit", (e) => {
  e.preventDefault();
  if (e.target.id === "computer-form") saveForm();
  if (e.target.id === "key-fix") {
    const key = $("input", e.target).value;
    if (key.startsWith("wrong")) {
      toast("Sunna is there, but the key doesn't match.");
      return;
    }
    if (!key) return;
    current().key = key;
    current().state = "checking";
    render();
    setTimeout(() => {
      current().state = "ready";
      render();
    }, 600);
  }
});
app.addEventListener("input", (e) => {
  if (e.target.closest("#computer-form")) {
    form[e.target.name] = e.target.value;
    if (e.target.name !== "name") checkForm();
    else {
      $(".never span") && ($(".never span").textContent = e.target.value || "workstation");
    }
  }
  if (e.target.dataset.own) {
    cfg[e.target.dataset.own] = e.target.value;
    if (ui.saveError) toast("io: permission denied");
  }
});
app.addEventListener("change", (e) => {
  if (e.target.dataset.setting) {
    cfg[e.target.dataset.setting] = e.target.value;
    videoChanged();
  }
});
function back() {
  if (ui.scan) {
    ui.scan = false;
    render();
    return;
  }
  if (ui.remove) {
    ui.remove = false;
    render();
    return;
  }
  if (ui.test) {
    ui.test = false;
    updateTest();
    return;
  }
  if (ui.context) {
    ui.context = false;
    render();
    return;
  }
  if (ui.submenu) {
    ui.submenu = "";
    render();
    return;
  }
  if (cur === "menu") {
    go("session");
    return;
  }
  if (ui.keyboard) {
    ui.keyboard = false;
    render();
    return;
  }
  if (cur === "session") {
    if (phone) go("menu");
    else leave();
    return;
  }
  if (cur === "connecting") leave("", false);
  else if (cur === "computer") go("overview");
  else if (["settings", "add", "edit", "find", "qr", "licenses"].includes(cur)) {
    const fallback = { find: "add", licenses: "settings", qr: ui.qrSelf ? "settings" : "computer" };
    go(parents[cur] || fallback[cur] || "computer", { back: true });
  } else {
    ui.remove = false;
    go("computer");
  }
}
addEventListener("keydown", (e) => {
  const input = e.target.matches("input,textarea,select");
  if (ui.context && ["ArrowDown", "ArrowUp"].includes(e.key)) {
    e.preventDefault();
    const buttons = $$(".context button:not(:disabled)"),
      at = buttons.indexOf(document.activeElement);
    buttons[
      (at + (e.key === "ArrowDown" ? 1 : buttons.length - 1) + buttons.length) % buttons.length
    ]?.focus();
    return;
  }
  if (e.key === "Escape") {
    e.preventDefault();
    back();
    return;
  }
  if (input) return;
  const mod = e.metaKey || e.ctrlKey;
  if (e.ctrlKey && e.altKey && ["session", "menu"].includes(cur)) {
    const a = {
      m: cur === "menu" ? "close-menu" : "menu",
      s: "stats",
      f: "fullscreen",
      q: "disconnect",
      g: "capture",
    }[e.key.toLowerCase()];
    if (a) {
      e.preventDefault();
      if (a === "capture") toggleSetting("capture");
      else act(a);
      return;
    }
  }
  if (mod) {
    const k = e.key.toLowerCase();
    if (k === "n") {
      e.preventDefault();
      openForm();
    } else if (k === ",") {
      e.preventDefault();
      go("settings");
    } else if (k === "e") {
      e.preventDefault();
      openForm(true);
    } else if (k === "r") {
      e.preventDefault();
      current().state = "checking";
      render();
      setTimeout(() => {
        current().state = "ready";
        render();
      }, 600);
    } else if (/^[1-9]$/.test(k) && C[+k - 1]) {
      e.preventDefault();
      sel = +k - 1;
      connect();
    } else if (e.key === "Backspace") {
      e.preventDefault();
      act("remove");
    }
    return;
  }
  if (["computer", "overview"].includes(cur)) {
    if (["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown"].includes(e.key)) {
      e.preventDefault();
      select(Math.max(0, Math.min(C.length, sel + (["ArrowLeft", "ArrowUp"].includes(e.key) ? -1 : 1))));
    }
    if (e.key === "Enter") {
      e.preventDefault();
      connect();
    }
    if (e.key === "Delete" || e.key === "Backspace") {
      e.preventDefault();
      act("remove");
    }
    if (e.key === "ContextMenu" || (e.shiftKey && e.key === "F10")) {
      e.preventDefault();
      act("context");
    }
  }
});
let drag = null,
  dragged = false,
  longTimer = 0,
  touches = new Map(),
  pinchStart = 0;
function bindGestures() {} // Listeners live on the persistent root and survive view changes.
app.addEventListener("pointerdown", (e) => {
  if (phone && cur === "session") wakeButtons();
  if (
    e.target.closest(
      "#test-root,input,select,.act,.toolbar,.st-tabs,.context,.session-controls,.keyboard-panel",
    )
  )
    return;
  touches.set(e.pointerId, { x: e.clientX, y: e.clientY });
  if (touches.size === 3 && cur === "session") {
    ui.threeFinger = true;
  }
  if (touches.size === 2) {
    const a = [...touches.values()];
    pinchStart = Math.hypot(a[0].x - a[1].x, a[0].y - a[1].y);
  }
  if (e.target.closest(".name-row") && phone)
    longTimer = setTimeout(() => {
      ui.context = true;
      render();
    }, 500);
  const float = e.target.closest(".float-buttons");
  if (e.target.closest("button") && !float) return;
  const track = $(".st-track");
  drag = {
    x: e.clientX,
    y: e.clientY,
    base: float ? float.offsetLeft : track ? new DOMMatrix(getComputedStyle(track).transform).m41 : 0,
    kind: float ? "float" : track ? "stage" : e.target.closest(".sess-shot") ? "pointer" : "",
    moved: false,
  };
});
app.addEventListener("pointermove", (e) => {
  if (touches.has(e.pointerId)) touches.set(e.pointerId, { x: e.clientX, y: e.clientY });
  if (touches.size === 2 && pinchStart) {
    const a = [...touches.values()],
      distance = Math.hypot(a[0].x - a[1].x, a[0].y - a[1].y);
    if (cur === "computer" && distance < pinchStart * 0.75) {
      drag = null;
      go("overview");
      pinchStart = 0;
    } else if (cur === "session") {
      ui.zoom = Math.max(1, Math.min(3, distance / pinchStart));
      const s = $(".sess-shot");
      s.style.transform = `scale(${ui.zoom})`;
      s.style.transformOrigin = "center";
    }
    return;
  }
  if (!drag) return;
  let dx = e.clientX - drag.x,
    dy = e.clientY - drag.y;
  if (Math.abs(dx) + Math.abs(dy) > 6) {
    drag.moved = true;
    clearTimeout(longTimer);
    if (drag.kind === "stage" || drag.kind === "float") app.setPointerCapture(e.pointerId);
  }
  if (drag.kind === "stage" && drag.moved) {
    const t = $(".st-track");
    if (t) {
      t.style.transition = "none";
      t.getAnimations().forEach((a) => a.cancel());
      t.style.transform = `translateX(${drag.base + dx * (sel === 0 && dx > 0 ? 0.3 : 1)}px)`;
    }
  } else if (drag.kind === "float") {
    const f = $(".float-buttons");
    ui.floatX = Math.max(8, Math.min(app.clientWidth - f.offsetWidth - 8, drag.base + dx));
    f.style.transform = `translateX(${ui.floatX - f.offsetLeft}px)`;
  } else if (drag.kind === "pointer" && cur === "session") {
    const p = $(".pointer"),
      s = $(".sess-shot");
    if (p && s) {
      const r = s.getBoundingClientRect(),
        x = cfg.touch === "Touch" ? e.clientX - r.left : +(p.dataset.x || p.offsetLeft) + e.movementX * 1.6,
        y = cfg.touch === "Touch" ? e.clientY - r.top : +(p.dataset.y || p.offsetTop) + e.movementY * 1.6;
      movePointer(p, x, y);
    }
  }
});
function movePointer(p, x, y) {
  const s = $(".sess-shot");
  x = Math.max(0, Math.min(s.offsetWidth - 15, x));
  y = Math.max(0, Math.min(s.offsetHeight - 21, y));
  p.dataset.x = x;
  p.dataset.y = y;
  p.style.transform = `translate(${x - p.offsetLeft}px,${y - p.offsetTop}px)`;
}
function pointerUp(e) {
  clearTimeout(longTimer);
  touches.delete(e.pointerId);
  if (!touches.size) {
    pinchStart = 0;
    if (ui.threeFinger) {
      ui.threeFinger = false;
      drag = null;
      ui.keyboard = !ui.keyboard;
      render();
      return;
    }
  }
  if (!drag) return;
  const d = drag;
  drag = null;
  if (d.kind === "float") {
    const f = $(".float-buttons");
    f.style.left = ui.floatX + "px";
    f.style.transform = "none";
  }
  if (d.moved) {
    dragged = true;
    setTimeout(() => (dragged = false), 80);
  }
  if (d.kind === "stage" && d.moved) {
    let dx = e.clientX - d.x;
    select(Math.max(0, Math.min(C.length, sel + (dx < -48 ? 1 : dx > 48 ? -1 : 0))));
  } else if (d.kind === "pointer" && !d.moved && cur === "session") {
    if (cfg.touch === "Touch") {
      const p = $(".pointer"),
        r = $(".sess-shot").getBoundingClientRect();
      if (p) movePointer(p, e.clientX - r.left, e.clientY - r.top);
    }
    const r = app.getBoundingClientRect(),
      ring = document.createElement("i");
    ring.className = "tap-ring";
    ring.style.left = e.clientX - r.left - 18 + "px";
    ring.style.top = e.clientY - r.top - 18 + "px";
    app.append(ring);
    const a = animate(
      ring,
      [
        { transform: "scale(.3)", opacity: 0.6 },
        { transform: "scale(1)", opacity: 0 },
      ],
      "move",
    );
    if (a) a.finished.then(() => ring.remove()).catch(() => ring.remove());
    else ring.remove();
  }
}
app.addEventListener("pointerup", pointerUp);
app.addEventListener("pointercancel", pointerUp);
addEventListener("resize", () => {
  ui.context = false;
  render();
  updateTest();
});
addEventListener("popstate", () => {
  suppressHistory = true;
  back();
  suppressHistory = false;
  history.pushState({ view: cur }, "");
});
history.replaceState({ sunna: true }, "");
history.pushState({ view: cur }, "");
setInterval(() => {
  if (ui.still || reduced() || !cfg.previews) return;
  const clocks = $$(".live .mac .mb .r,.live .gn .gb span:nth-child(2),.live .w11 .tk span");
  clocks.forEach((el) => {
    el.textContent = new Date().toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  });
}, 60000);
cfg.openSessions = "Full screen";
