const packageCommands = Object.freeze({
  aptXfce: 'sudo apt install xvfb x11-utils dbus-x11 xfce4',
  aptPlasma: 'sudo apt install xvfb x11-utils dbus-x11 plasma-workspace',
  aptPlasmaX11: 'sudo apt install xvfb x11-utils dbus-x11 plasma-session-x11',
  pacmanXfce: 'sudo pacman -S xorg-server-xvfb xorg-xdpyinfo dbus xfce4',
  pacmanPlasma: 'sudo pacman -S xorg-server-xvfb xorg-xdpyinfo dbus plasma-x11-session',
  dnfXfce: 'sudo dnf install xorg-x11-server-Xvfb xdpyinfo dbus-x11 @xfce-desktop-environment',
  dnfPlasma: 'sudo dnf install xorg-x11-server-Xvfb xdpyinfo dbus-x11 plasma-workspace-x11',
});

export async function wireSharing(sheet, {invoke, listen, copy, qr, toast}) {
  const find = selector => sheet.querySelector(selector);
  const toggle = find('#share-host');
  const actions = find('.share-actions');
  const linkRow = find('.link-row');
  let lastActionError, confirmed;
  let current, pending = false, confirming = false, generation = 0;
  function button(label, run) {
    const b = document.createElement('button');
    b.className = 'button secondary small';
    b.textContent = label;
    b.onclick = run;
    actions.append(b);
  }
  function paint(s) {
    current = s;
    if (s.state === 'off') confirmed = false;
    if (s.state === 'on' || s.state === 'unavailable' || (s.state === 'needs-permission' && !pending)) confirmed = true;
    toggle.checked = ['error','stopping'].includes(s.state) ? (confirmed ?? s.state === 'stopping') : s.state !== 'off';
    toggle.disabled = pending || confirming || !s.supported || s.managed_by === 'system' || ['installing','starting','stopping'].includes(s.state);
    const message = {off:'Off', installing:'Installing the host…', starting:'Starting…', stopping:'Stopping…',
      'needs-permission':'Waiting for permission…', on:'Sharing', error:''};
    let status = message[s.state];
    if (s.state === 'off' && s.starts_by_itself) status = s.managed_by === 'system' ? 'Off until the next boot' : 'Off until you next log in';
    if (s.state === 'on' && s.address && s.reach) status += ' on ' + (s.reach === 'tailscale' ? 'your Tailscale network' : 'your local network') + ' · ' + s.address;
    if (s.state === 'on' && s.address && !s.reach) status += ' · ' + s.address;
    if (s.viewer) status += ' · ' + s.viewer + ' is connected';
    if (s.reason && s.state !== 'stopping') status = (s.state === 'on' ? 'Not sharing · ' : '') + s.reason;
    const unmet = s.permissions.filter(p => !p.granted);
    let greeter;
    if (s.managed_by === 'system' && s.state !== 'off') {
      greeter = s.shows_login_screen == null ? 'Login screen sharing hasn’t been checked yet.'
        : s.shows_login_screen ? 'The login screen can be shared.' : 'The login screen couldn’t be shared.';
    }
    const parts = [status, greeter, ...unmet.map(p=>p.label), s.fix?.label].filter(Boolean);
    find('.this-sharing-text').textContent = parts.map((part, i) => i < parts.length - 1 && !/[.…!?]$/.test(part) ? part + '.' : part).join(' ');
    find('.this-sharing').dataset.state = s.state;
    actions.replaceChildren();
    const command = s.managed_by === 'system'
      ? (s.state !== 'off' || s.starts_by_itself ? 'sudo systemctl disable --now sunna-host.service' : 'sudo systemctl enable --now sunna-host.service')
      : packageCommands[s.fix?.command];
    if (typeof command === 'string') {
      const code = document.createElement('code');
      code.textContent = command;
      actions.append(code);
      button('Copy command', e=>copy(command,e.currentTarget));
    }
    for (const permission of unmet.filter(p=>p.id !== 'portal')) {
      button(permission.id === 'screen' ? 'Screen Recording…' : 'Accessibility…',
        ()=>invoke('sharing_open_permission',{which:permission.id}).catch(e=>toast(String(e))));
    }
    if (confirming) {
      find('.this-sharing-text').textContent = (s.viewer || 'Someone') + ' is connected. Stop sharing anyway?';
      button('Keep sharing', ()=>{confirming=false;paint(current);toggle.focus();});
      button('Stop sharing', ()=>change(false));
    }
    actions.hidden = !actions.childElementCount;
    linkRow.hidden = !s.link || s.state !== 'on';
    find('.link-text').textContent = linkRow.hidden ? '' : 'sunna://' + s.address + '?key=••••';
  }
  async function refresh() {
    const before = generation;
    const s = await invoke('sharing_status');
    if (before === generation && sheet.isConnected && !confirming && !pending &&
        !(lastActionError && s.state === (confirmed ? 'on' : 'off')) && JSON.stringify(s) !== JSON.stringify(current)) {
      lastActionError = false;
      paint(s);
    }
    return s;
  }
  async function change(on) {
    confirming = false;
    lastActionError = false;
    generation++;
    pending = true;
    paint(current);
    try {
      const result = await invoke('sharing_set',{on});
      lastActionError = result.state === 'error';
      paint(result);
    }
    catch(e) { toast(String(e)); }
    finally { pending = false; paint(current); toggle.focus(); }
  }
  toggle.onchange = async () => {
    const on = toggle.checked;
    toggle.disabled = true;
    try {
      if (!on && (await refresh()).viewer) {confirming=true;paint(current);return;}
      await change(on);
    } catch(e) {toast(String(e));paint(current);}
  };
  find('.copy-link').onclick = e=>copy(current.link,e.currentTarget);
  find('.show-qr').onclick = e=>qr(current.link,e.currentTarget);
  find('.network-note').onclick = ()=>invoke('open_network_security').catch(e=>toast(String(e)));
  const unlisten = await listen('sharing-changed', ({payload})=>{generation++; if(sheet.isConnected) paint(payload);});
  async function poll() {
    if (!sheet.isConnected) {unlisten();return;}
    try { if (!pending) await refresh(); }
    catch {find('.this-sharing-text').textContent = 'Couldn’t read sharing settings.';}
    setTimeout(poll, 2500);
  }
  await poll();
}
