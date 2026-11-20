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
  "100.124.64.79": { name: "debian", os: "Debian GNU/Linux 13", device: "desktop", width: 2560, height: 1600, rtt: 11.8 },
  "100.111.171.4": { name: "macbook", os: "macOS 26.0", device: "laptop", model: "MacBook Air", width: 2940, height: 1912, rtt: 6.2 },
  "100.80.206.99": { name: "ubuntu", os: "Ubuntu 22.04.5 LTS", device: "vm", width: 1920, height: 1080, rtt: 38.4 },
  "100.90.1.7": { name: "studio", os: "Debian GNU/Linux 13", device: "desktop", width: 3440, height: 1440, rtt: 14.1, busy: true },
};

let machines =
  params.get("scene") === "empty"
    ? []
    : [
        { id: "a1", name: "debian", address: "100.124.64.79", key: KEY, os: "Debian GNU/Linux 13", device: "desktop", model: "", width: 2560, height: 1600, added: 0, last_seen: now(), last_connected: 0 },
        { id: "m1", name: "macbook", address: "100.111.171.4", key: KEY, os: "macOS 26.0", device: "laptop", model: "MacBook Air", width: 2940, height: 1912, added: 0, last_seen: now(), last_connected: 0 },
        { id: "s1", name: "studio", address: "100.90.1.7", key: KEY, os: "Debian GNU/Linux 13", device: "desktop", model: "", width: 3440, height: 1440, added: 0, last_seen: now(), last_connected: 0 },
        { id: "u1", name: "ubuntu", address: "100.80.206.99", key: "an-old-key", os: "", device: "", model: "", width: 0, height: 0, added: 0, last_seen: 0, last_connected: 0 },
        { id: "w1", name: "windows", address: "100.70.3.3", key: KEY, os: "Windows 11", device: "desktop", model: "", width: 2560, height: 1440, added: 0, last_seen: now() - 3 * 3600, last_connected: 0 },
      ];

const original = [...machines];
const count = params.has('count') ? Math.max(0, Math.min(20,Number(params.get('count')))) : machines.length;
// The first four entries have saved pictures from real Linux sessions.
const samples = [
  { name: 'debian', device: 'desktop', os: 'Debian GNU/Linux 13', width: 2560, height: 1600, picture: '05-mate-planning' },
  { name: 'thinkpad', device: 'laptop', os: 'Debian GNU/Linux 13', width: 1920, height: 1080, picture: '02-gnome-notes' },
  { name: 'studio', device: 'mini', os: 'Debian GNU/Linux 13', width: 3440, height: 1440, picture: '03-plasma-krita' },
  { name: 'ubuntu', device: 'vm', os: 'Ubuntu', width: 1920, height: 1080, picture: '04-xfce-docs' },
  { name: 'macbook', device: 'laptop', model: 'MacBook Air', os: 'macOS 26.0', width: 2940, height: 1912 },
  { name: 'windows', device: 'desktop', os: 'Windows 11', width: 1920, height: 1280 },
];
const states = ['ready','ready','busy','ready','unreachable','wrong-key'];
if (params.has('count')) machines = Array.from({ length: count }, (_, i) => {
  const sample = samples[i % samples.length];
  const suffix = i >= samples.length ? ` ${i + 1}` : '';
  return {
    ...original[i % original.length], id: `c${i}`,
    name: params.has('long') ? `Design and engineering workstation ${i+1} — San Francisco` : sample.name + suffix,
    device: sample.device, os: sample.os, width: sample.width, height: sample.height,
    model: sample.model || '', key: KEY, kind: '', last_seen: sample.name === 'macbook' ? now()-10800 : now(),
  };
});
const pictures = new Map(machines.map((machine, i) => {
  const sample = params.has('count') ? samples[i % samples.length] : samples.find(entry => entry.name === machine.name);
  return [machine.id, sample?.picture];
}));
if(params.has('kind')) machines.forEach(m=>m.device=params.get('kind'));
if(params.has('never')) machines.forEach(m=>m.last_seen=0);
let settings = { view: params.get('view') || 'gallery', key: KEY, windowed: false, stats: true, menu_button: true, audio: true, pictures: params.get('pictures') || 'sharp' };

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
    viewer: found.busy ? "MacBook Pro" : "",
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

// ?app-update=up-to-date|checking|downloading|ready|failed|unmanaged
// ?host-update=available|updating|done|failed|unsupported|unmanaged
// ?host-result=failed makes the confirmed request fail. Fixtures never use real keys.
const appMode = params.get('app-update') || 'up-to-date';
let appUpdate = {version:'0.1.0',current:'0.1.0',last_check:now(),available:null,pending:null,previous:null,failed:[],healthy:true,starts:1,policy:'notify',managed:true,message:null,phase:'idle',error:null,latest_version:appMode==='up-to-date'?'0.1.0':'0.2.0',policy_locked:false};
if(appMode==='checking')appUpdate.phase='checking';
if(appMode==='downloading')Object.assign(appUpdate,{phase:'downloading',available:'0.2.0'});
if(appMode==='ready')Object.assign(appUpdate,{available:'0.2.0',pending:'0.2.0'});
if(appMode==='failed')appUpdate.error='Could not verify the release signature. This copy has not changed.';
if(appMode==='unmanaged')Object.assign(appUpdate,{managed:false,message:'This copy cannot update itself. Install a signed download first. For a system package, use apt, dnf or pacman.'});
const hostModes = new Map(), hostPolicies = new Map();
const hostMode = address => hostModes.get(address) || params.get('host-update');
function hostUpdate(address) {
  const mode=hostMode(address);
  if(!mode || mode==='unsupported')return null;
  return {version:mode==='done'?'0.2.0':'0.1.0',supported:mode!=='unmanaged',policy:hostPolicies.get(address)||'notify',state:{current:mode==='done'?'0.2.0':'0.1.0',previous:mode==='done'?'0.1.0':null,available:['available','updating'].includes(mode)?'0.2.0':null,pending:mode==='updating'?'0.2.0':null,failed:mode==='failed'?['0.2.0']:[],healthy:true,last_check:now()}};
}
let sharingState = params.get('sharing') || 'off';
function sharingSnapshot() {
  const mode = sharingState;
  const on = ['on','busy','system'].includes(mode);
  return { supported:mode !== 'unsupported', state:on?'on':mode === 'unsupported'?'off':mode,
    managed_by:mode === 'system'?'system':'app', mode:'desktop', reach:'lan',
    address:on?'192.168.1.12:48800':null, link:on?'sunna://192.168.1.12:48800?key='+KEY:null,
    viewer:mode==='busy'?"MacBook":null, starts_by_itself:on, shows_login_screen:mode==='system',
    permissions:mode==='needs-permission'?[{id:'portal',granted:false,label:'Choose your screen, allow remote interaction, and remember the selection when your desktop asks.'}]:[],
    reason:mode==='error'?'The background desktop needs system packages.':mode==='unsupported'?'This Wayland desktop can’t share its screen.':null,
    fix:mode==='error'?{label:'Install the desktop packages.',command:'aptXfce'}:mode==='system'?{label:'An administrator manages login-screen sharing.',command:'sudo systemctl disable --now sunna-host.service'}:null };
}
const commands = {
  sharing_status: sharingSnapshot,
  sharing_set: async ({on}) => {
    for(const phase of on?['installing','starting','on']:['stopping','off']) {
      sharingState=phase;emit('sharing-changed',sharingSnapshot());await delay(120);
    }
    return sharingSnapshot();
  },
  sharing_open_permission: () => {},
  open_network_security: () => {},
  update_status: () => structuredClone(appUpdate),
  update_check: async () => {
    appUpdate.phase='checking';appUpdate.error=null;await delay(1200);
    appUpdate.phase='downloading';appUpdate.available='0.2.0';await delay(1800);
    appUpdate.phase='idle';appUpdate.pending='0.2.0';return structuredClone(appUpdate);
  },
  update_policy: ({policy}) => {appUpdate.policy=policy;},
  update_restart: () => {appUpdate.current=appUpdate.version=appUpdate.pending;appUpdate.pending=null;appUpdate.available=null;},
  host_update: async ({address,action}) => {
    await delay(250);
    const mode=hostMode(address);
    if(!mode || mode==='unsupported')throw 'This host is too old to support remote updates. Install a signed download once by hand.';
    if(action==='update') {
      if(params.get('host-result')==='failed')throw 'The host could not save the update request: disk is full.';
      hostModes.set(address,'updating');setTimeout(()=>hostModes.set(address,'done'),6500);
    } else if(['auto','notify','off'].includes(action))hostPolicies.set(address,action);
    return hostUpdate(address);
  },
  // Just a preview; the Rust command makes the scannable code.
  qr_code: () => {
    let path = "";
    for (let y = 0; y < 29; y++) {
      for (let x = 0; x < 29; x++) {
        const finder = [[0, 0], [22, 0], [0, 22]].find(([left, top]) =>
          x >= left - 1 && x <= left + 7 && y >= top - 1 && y <= top + 7);
        let dark;
        if (finder) {
          const dx = x - finder[0], dy = y - finder[1];
          dark = dx >= 0 && dx < 7 && dy >= 0 && dy < 7 &&
            (dx === 0 || dx === 6 || dy === 0 || dy === 6 || (dx >= 2 && dx <= 4 && dy >= 2 && dy <= 4));
        } else {
          dark = (x * y + x + y * 3) % 7 < 3;
        }
        if (dark) path += `M${x + 4} ${y + 4}h1v1h-1z`;
      }
    }
    return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 37 37" shape-rendering="crispEdges"><rect width="37" height="37" fill="#fff"/><path d="${path}" fill="#000"/></svg>`;
  },
  app_info: () => ({ version: appUpdate.version, protocol: 8, computer: "macbook", platform: "macos" }),
  get_settings: () => ({ ...settings }),
  // ?access=no previews the Accessibility prompt flow.
  keyboard_access: () => !location.search.includes("access=no"),
  allow_keyboard_access: () => {},
  reset_keyboard_access: () => {},
  set_key: ({key}) => { settings.key = key; },
  set_settings: ({ settings: next }) => {
    settings = { ...next, key: settings.key };
  },
  list_machines: () => machines.map((machine) => ({ ...machine })),
  machine_picture: async ({id}) => {
    if(params.has('no-picture')) return null;
    const file = pictures.get(id);
    if (!file) return null;
    if(settings.pictures==='off') return null;
    const blob=await (await fetch(`../dev/design/frames/${file}.jpg`)).blob();
    if(settings.pictures!=='blurred') return [...new Uint8Array(await blob.arrayBuffer())];
    // What the viewer keeps when blurred: the screen averaged down to 32 pixels across.
    const bitmap=await createImageBitmap(blob), canvas=new OffscreenCanvas(32,Math.max(1,Math.round(32*bitmap.height/bitmap.width)));
    canvas.getContext('2d').drawImage(bitmap,0,0,canvas.width,canvas.height);
    return [...new Uint8Array(await (await canvas.convertToBlob({type:'image/jpeg',quality:.9})).arrayBuffer())];
  },
  machine_statuses: async () => {
    if(params.has('count')) return Object.fromEntries(machines.map((m,i)=>[m.id,{update:hostUpdate(m.address),state:params.get('state')||states[i%6],os:m.os,device:m.device,width:m.width,height:m.height,rtt_ms:[4,11,14,38,0,0][i%6],viewer:i%6===2?"MacBook Pro":"",detail:params.get('state')==='not-sharing'?'Nobody is signed in to the desktop.':''}]));
    const results = await Promise.all(machines.map((machine) => check(machine.address, machine.key)));
    return Object.fromEntries(machines.map((machine, index) => [machine.id, {...results[index],update:hostUpdate(machine.address)}]));
  },
  check_machine: async ({ address, key }) => ({...await check(address, key),update:hostUpdate(address)}),
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
      last_connected: 0, kind: draft.kind || "",
    };
    machines.push(machine);
    return { ...machine };
  },
  update_machine: ({ id, draft }) => {
    const machine = machines.find((entry) => entry.id === id);
    Object.assign(machine, { name: draft.name || machine.name, address: draft.address, key: draft.key, kind: draft.kind || "" });
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
    return { this_device: "macbook", machines: [...sharing, ...others] };
  },
  this_computer: async () => {
    await delay(500);
    return { name: "macbook", address: "100.111.171.4", key: settings.key, sharing: "unreachable" };
  },
  connect: async ({ id }) => {
    window.__sunnaMock.session = id;
    const fail = params.has("fail");
    setTimeout(() => {if(window.__sunnaMock.session===id) emit("session-progress", { id, phase: "video" });}, 700);
    if (fail) {
      setTimeout(() => emit("session-ended", { id, opened: false, reason: "This computer can't play that computer's video (no decoder for codec \"hevc\")." }), 1500);
      return;
    }
    setTimeout(() => {if(window.__sunnaMock.session===id) emit("session-progress", { id, phase: "open" });}, 1400);
    setTimeout(() => emit("session-ended", { id, opened: true, reason: null }), 3600);
  },
  cancel_connect: () => { const id=window.__sunnaMock.session;window.__sunnaMock.session=null;emit("session-ended",{id,opened:false,reason:null}); },
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

window.__sunnaMock = { commands, emit, session: null };
