// A stand-in for the Rust side, so the launcher UI can be worked on in a
// plain browser (the app's webview never loads this):
//
//   python3 -m http.server 8766 -d apps/sunna   →   localhost:8766/ui/
//
// ?scene=empty starts with no computers; ?time=19:30 sets the sky;
// ?fail=1 makes connecting fail.

const params = new URLSearchParams(location.search);
const listeners = new Map();
const emit = (name, payload) => {
  for (const callback of listeners.get(name) ?? []) callback({ payload });
};
const delay = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const now = () => Math.floor(Date.now() / 1000);
const KEY = "0123456789abcdef0123456789abcdef";

// Who answers where, and what they are.
const HOSTS = {
  "100.124.64.79": { name: "archlinux", os: "Arch Linux", device: "desktop", width: 2560, height: 1440, rtt: 11.8 },
  "100.111.171.4": { name: "Sunny's MacBook Air", os: "macOS 26.0", device: "laptop", model: "MacBook Air", width: 2940, height: 1912, rtt: 6.2 },
  "100.80.206.99": { name: "ubuntu", os: "Ubuntu 22.04.5 LTS", device: "vm", width: 1920, height: 1080, rtt: 38.4 },
  "100.90.1.7": { name: "studio", os: "Fedora Linux 42", device: "desktop", width: 3440, height: 1440, rtt: 14.1, busy: true },
};

let machines =
  params.get("scene") === "empty"
    ? []
    : [
        { id: "a1", name: "archlinux", address: "100.124.64.79", key: KEY, os: "Arch Linux", device: "desktop", model: "", width: 2560, height: 1440, added: 0, last_seen: now(), last_connected: 0 },
        { id: "m1", name: "MacBook Air", address: "100.111.171.4", key: KEY, os: "macOS 26.0", device: "laptop", model: "MacBook Air", width: 2940, height: 1912, added: 0, last_seen: now(), last_connected: 0 },
        { id: "s1", name: "studio", address: "100.90.1.7", key: KEY, os: "Fedora Linux 42", device: "desktop", model: "", width: 3440, height: 1440, added: 0, last_seen: now(), last_connected: 0 },
        { id: "u1", name: "ubuntu", address: "100.80.206.99", key: "an-old-key", os: "", device: "", model: "", width: 0, height: 0, added: 0, last_seen: 0, last_connected: 0 },
        { id: "w1", name: "gaming-pc", address: "100.70.3.3", key: KEY, os: "Windows 11", device: "desktop", model: "", width: 2560, height: 1440, added: 0, last_seen: now() - 3 * 3600, last_connected: 0 },
      ];

let settings = { key: KEY, windowed: false, stats: true, menu_button: true };

function hostOf(address) {
  const text = address.trim().split(/[?/#]/)[0];
  if (text.startsWith("[")) return text.slice(1, text.indexOf("]"));
  return (text.match(/:/g) || []).length === 1 ? text.split(":")[0] : text;
}

async function check(address, key) {
  const host = hostOf(address);
  await delay(250 + Math.random() * 450);
  if (!host || /\s/.test(host)) return { state: "invalid", detail: "That doesn't look like an address." };
  if (!/^[\d.]+$/.test(host) && !Object.values(HOSTS).some((entry) => entry.name === host)) {
    return { state: "not-found", detail: `Couldn't find a computer called “${host}”.` };
  }
  const ip = Object.keys(HOSTS).find((ip) => ip === host || HOSTS[ip].name === host);
  const found = HOSTS[ip];
  const resolved = `${ip ?? host}:48800`;
  if (!found) {
    await delay(900);
    return { state: "unreachable", resolved, detail: `Nothing answered at ${resolved}. Is Sunna sharing on that computer?` };
  }
  if (key !== KEY) {
    return { state: "wrong-key", resolved, rtt_ms: found.rtt, detail: "Sunna is there, but the key doesn't match." };
  }
  return {
    state: found.busy ? "busy" : "ready",
    name: found.name,
    os: found.os,
    device: found.device,
    model: found.model ?? "",
    width: found.width,
    height: found.height,
    rtt_ms: found.rtt + Math.random() * 3,
    resolved,
    detail: found.busy ? "Someone is connected to it right now." : "",
  };
}

const commands = {
  app_info: () => ({ version: "0.0.1", protocol: 3, user: "Sunny", computer: "Sunny's MacBook Air", platform: "macos" }),
  get_settings: () => ({ ...settings }),
  set_settings: ({ settings: next }) => {
    settings = { ...next };
  },
  list_machines: () => machines.map((machine) => ({ ...machine })),
  machine_statuses: async () => {
    const results = await Promise.all(machines.map((machine) => check(machine.address, machine.key)));
    return Object.fromEntries(machines.map((machine, index) => [machine.id, results[index]]));
  },
  check_machine: ({ address, key }) => check(address, key),
  add_machine: ({ draft }) => {
    const about = draft.about ?? {};
    const machine = {
      id: Math.random().toString(16).slice(2, 10),
      name: draft.name || about.name || hostOf(draft.address),
      address: draft.address,
      key: draft.key,
      os: about.os ?? "",
      device: about.device ?? "",
      model: about.model ?? "",
      width: about.width ?? 0,
      height: about.height ?? 0,
      added: now(),
      last_seen: 0,
      last_connected: 0,
    };
    machines.push(machine);
    return { ...machine };
  },
  update_machine: ({ id, draft }) => {
    const machine = machines.find((entry) => entry.id === id);
    Object.assign(machine, { name: draft.name || machine.name, address: draft.address, key: draft.key });
    return { ...machine };
  },
  remove_machine: ({ id }) => {
    machines = machines.filter((machine) => machine.id !== id);
  },
  reorder_machines: () => {},
  scan_tailscale: async () => {
    await delay(900);
    const sharing = Object.entries(HOSTS).map(([ip, host]) => ({
      name: host.name,
      dns_name: `${host.name}.tail0000.ts.net`,
      ip,
      os: host.os,
      device: host.device,
      state: ip === "100.80.206.99" ? "wrong-key" : host.busy ? "busy" : "ready",
    }));
    const others = [
      { name: "pixel-8", ip: "100.101.4.2", os: "Android", state: "not-sharing" },
      { name: "moto g52", ip: "100.101.4.3", os: "Android", state: "offline" },
      { name: "alpine", ip: "100.101.4.9", os: "Linux", state: "offline" },
    ].map((entry) => ({ dns_name: "", device: "", ...entry }));
    return { this_device: "sunnys-macbook-air-3", machines: [...sharing, ...others] };
  },
  this_computer: async () => {
    await delay(500);
    return { name: "Sunny's MacBook Air", address: "100.111.171.4", key: settings.key, sharing: "unreachable" };
  },
  connect: async ({ id }) => {
    const fail = params.has("fail");
    setTimeout(() => emit("session-progress", { id, phase: "video" }), 700);
    if (fail) {
      setTimeout(() => emit("session-ended", { id, opened: false, reason: "This computer can't play that computer's video (no decoder for codec \"hevc\")." }), 1500);
      return;
    }
    setTimeout(() => emit("session-progress", { id, phase: "open" }), 1400);
    setTimeout(() => emit("session-ended", { id, opened: true, reason: null }), 3600);
  },
  cancel_connect: () => {},
};

window.__TAURI__ = {
  core: {
    invoke: async (name, args = {}) => {
      const command = commands[name];
      if (!command) throw new Error(`mock: no command ${name}`);
      return command(args);
    },
  },
  event: {
    listen: async (name, callback) => {
      listeners.set(name, [...(listeners.get(name) ?? []), callback]);
      return () => {};
    },
  },
};
