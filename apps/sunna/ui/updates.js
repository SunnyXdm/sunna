// Update presentation. Host replies acknowledge a request, not a completed update.
// Only a later probe reporting the new running version counts as success.
const INSTALL = 'curl -fsSL https://sunna.sunnydx.dev/install.sh | sh -s -- host';
const $ = (root, selector) => root.querySelector(selector);
const text = (root, selector, value) => { $(root, selector).textContent = value; };
export function newer(a, b) {
  // Releases use semver; don't compare 0.10 and 0.9 lexically.
  const parse = v => /^(\d+)\.(\d+)\.(\d+)(?:-([\w.-]+))?(?:\+[\w.-]+)?$/.exec(v || '');
  const x = parse(a), y = parse(b);
  if (!x || !y) return false;
  for (let i = 1; i <= 3; i++) if (+x[i] !== +y[i]) return +x[i] > +y[i];
  if (!x[4] || !y[4]) return !x[4] && !!y[4];
  const p = x[4].split('.'), q = y[4].split('.');
  for (let i = 0; i < Math.max(p.length, q.length); i++) {
    if (p[i] === q[i]) continue;
    if (p[i] == null || q[i] == null) return q[i] == null;
    const pn = /^\d+$/.test(p[i]), qn = /^\d+$/.test(q[i]);
    return pn && qn ? +p[i] > +q[i] : pn !== qn ? !pn : p[i] > q[i];
  }
  return false;
}

export function updateUI({invoke, state, render, openSheet, closeSheet, toast, menu}) {
  let app = null, localError = '', fetching = false, checking = false, restarting = false;
  const requests = new Map();
  const settings = new Set();
  const notice = document.getElementById('app-update-notice');
  const hostCall = (machine, action) => invoke('host_update', {record: machine.id, address: machine.address, key: machine.key, action});
  const identity = machine => `${machine.address}\n${machine.key}`;
  function hostInfo(machine, check) {
    const u = check?.update, s = u?.state || {};
    let request = requests.get(machine.id);
    if (request && request.identity !== identity(machine)) { requests.delete(machine.id); request = null; }
    if (request?.running || request?.error) {
      if (u && newer(u.version, request.before) && !s.pending) {
        request.running = false; request.error = null; request.done = u.version;
      } else if ((s.failed || []).some(v => !request.failed.includes(v))) {
        request.running = false; request.error = 'The host could not run the new release. It kept the previous version.';
      }
    }
    if (s.error) return {label:'Update failed', detail:s.error};
    if (request?.error) return {label:'Update failed', detail:request.error};
    if (request?.running || s.pending) {
      const waiting = s.pending ? 'The release is ready. Waiting for the host to restart when nobody is connected.' : 'The host has been asked to download and verify the signed release.';
      return {label:request && Date.now()-request.started > 120000 ? 'Still waiting…' : 'Updating…', detail: request && Date.now()-request.started > 120000 ? 'Still waiting for the new version. The host may be in use or unable to download. Check its connection and update log.' : waiting};
    }
    const known = app?.latest_version || app?.available || app?.pending;
    const latest = newer(known, s.available) || !s.available ? known : s.available;
    if (newer(latest, u?.version) && !(s.failed || []).includes(latest)) return {label:'Update available', detail:`Version ${latest} is available. Running ${u.version}.` + (u.supported === false ? ' Install a signed download once by hand to enable remote updates.' : '')};
    const failed = (s.failed || []).filter(v => newer(v, u?.version)).at(-1);
    if (failed) return {label:'Update failed', detail:`Release ${failed} failed on this computer. The host will wait for a newer release.`};
    if (request?.done) return {label:`Updated to ${request.done}`, detail:`Back on version ${request.done}.`};
    if (s.previous && newer(u?.version, s.previous) && !newer(s.available, u.version)) return {label:`Updated to ${u.version}`, detail:`Running version ${u.version}. Previously ${s.previous}.`};
    if (u?.supported === false) return {quiet:true, label:'Install once by hand', detail:'This host is not a managed installation. Install a signed download once to enable remote updates.'};
    if (!u && check && ['ready','busy','update-needed'].includes(check.state)) return {quiet:true, label:'Install once by hand', detail:'This host does not report update support. Install a signed download once to enable remote updates.'};
    return null;
  }
  const failedApp = () => (app?.failed || []).filter(v => newer(v, app?.version)).at(-1);
  function appText() {
    if (!app) return localError ? 'Could not read update status' : 'Checking update status…';
    if (app.packaged) return 'Updated by your system';
    if (!app.managed) return 'Cannot update this copy';
    if (app.phase === 'downloading') return 'Downloading and verifying…';
    if (checking || app.phase === 'checking') return 'Checking for updates…';
    if (app.pending) return `Version ${app.pending} is ready`;
    if (localError || app.error) return 'Could not update';
    if (app.available) return `Version ${app.available} is available`;
    if (failedApp()) return 'Could not update';
    return app.latest_version && !newer(app.latest_version, app.version) ? 'Up to date' : 'Not checked since opening';
  }
  function paint() {
    const waiting = outdated();
    notice.hidden = !!state.session || !((app?.managed && app?.pending) || waiting.length);
    notice.disabled = restarting;
    for (const sheet of settings) {
      if (!sheet.isConnected) {settings.delete(sheet); continue;}
      text(sheet, '.update-version', `Sunna ${app?.version || state.info.version || ''}`);
      text(sheet, '.update-state', appText());
      const error = localError || app?.error || (failedApp() && !app?.pending && !app?.available ? `Release ${failedApp()} failed. Waiting for a newer release.` : '');
      text(sheet, '.update-explanation', app?.message || error);
      $(sheet, '.update-explanation').hidden = !(app?.message || error);
      const busy = checking || (app?.phase && app.phase !== 'idle');
      $(sheet, '.update-check').disabled = !app?.managed || busy;
      $(sheet, '.update-restart').hidden = !app?.managed || !app?.pending;
      $(sheet, '.update-restart').disabled = restarting || !!state.session;
      const policy = $(sheet, '.app-update-policy');
      policy.value = app?.policy || 'notify';
      policy.disabled = !app?.managed || busy || app?.policy_locked;
      text(sheet, '.update-policy-hint', app?.packaged ? '' : app && !app.managed ? 'Available with a signed download.' : app?.policy_locked ? 'The SUNNA_UPDATE setting controls this choice.' : policy.value === 'off' ? 'No background checks. You can still check here.' : policy.value === 'auto' ? 'A downloaded update takes effect the next time you open Sunna.' : 'Checks daily and downloads in the background. You choose when to restart.');
    }
  }
  async function refresh() {
    if (fetching) return;
    fetching = true;
    try { app = await invoke('update_status'); }
    catch (e) { localError = String(e); }
    finally { fetching = false; paint(); if (!state.session) render(); }
  }
  async function restart() {
    if (state.session || restarting) return;
    restarting = true; paint();
    try { await invoke('update_restart'); }
    catch (e) { localError = String(e); toast(localError); }
    finally { restarting = false; paint(); }
  }
  // The computers a newer release is waiting for.
  function outdated() {
    return state.machines.filter(machine => hostInfo(machine, state.checks.get(machine.id))?.label === 'Update available');
  }
  notice.onclick = () => {
    const items = [];
    if (app?.managed && app?.pending) items.push({label:`Restart Sunna to update to ${app.pending}`, run:restart});
    for (const machine of outdated()) items.push({label:`Update ${machine.name}…`, run:() => showHost(machine, notice)});
    if (items.length) menu(items, notice);
  };
  function wireSettings(sheet) {
    settings.add(sheet); paint(); refresh();
    $(sheet, '.update-check').onclick = async () => {
      if (checking) return;
      checking = true; localError = ''; paint();
      try { app = await invoke('update_check'); }
      catch (e) { localError = String(e); }
      finally { checking = false; await refresh(); }
    };
    $(sheet, '.update-restart').onclick = restart;
    $(sheet, '.app-update-policy').onchange = async event => {
      const control = event.currentTarget, previous = app.policy;
      control.disabled = true; localError = '';
      try { await invoke('update_policy', {policy:control.value}); await refresh(); }
      catch (e) { control.value = previous; localError = String(e); paint(); }
    };
  }
  async function showHost(machine, opener) {
    if (state.session) return;
    const sheet = openSheet('host-update-template', opener);
    text(sheet, 'h2', `Update ${machine.name}?`);
    const confirm = $(sheet, '.confirm');
    $(sheet, '.cancel').onclick = () => closeSheet(sheet);
    $(sheet, '.cancel').focus();
    let info;
    try { info = await hostCall(machine, 'status'); }
    catch (e) {
      if (!sheet.isConnected) return;
      text(sheet, '.host-update-detail', String(e));
      // A network or authentication error isn't proof of an old installation.
      if (/support|older|version|managed/i.test(String(e))) showInstall(sheet);
      confirm.hidden = true; $(sheet, '.cancel').textContent = 'Done'; return;
    }
    if (!sheet.isConnected) return;
    if (!info.supported) {
      text(sheet, '.host-update-detail', 'This host is not a managed installation. Install a signed download once to enable remote updates.');
      showInstall(sheet); confirm.hidden = true; $(sheet, '.cancel').textContent = 'Done'; return;
    }
    const existing = requests.get(machine.id);
    text(sheet, '.host-update-detail', [state.checks.get(machine.id)?.state === 'update-needed' ? state.checks.get(machine.id).detail : '', hostInfo(machine, {update:info})?.detail || `Running version ${info.version}.`].filter(Boolean).join(' '));
    if (existing?.running || info.state?.pending) {
      confirm.hidden = true; $(sheet, '.cancel').textContent = 'Done'; return;
    }
    text(sheet, '.host-update-body', 'This computer downloads and verifies the signed release itself. Its host restarts when nobody is connected, then comes back in a moment.');
    confirm.disabled = false;
    confirm.onclick = async () => {
      confirm.disabled = true;
      const request = {identity:identity(machine), running:true, started:Date.now(), before:info.version, failed:info.state?.failed || []};
      requests.set(machine.id, request); render();
      await closeSheet(sheet);
      try {
        const reply = await hostCall(machine, 'update');
        const check = state.checks.get(machine.id);
        if (check) check.update = reply;
      } catch (e) { request.running = false; request.error = String(e); }
      render();
    };
  }
  function showInstall(sheet) {
    text(sheet, '.host-update-body', 'Install once by hand on that computer. On Linux, run:');
    text(sheet, '.host-install-command', INSTALL);
    $(sheet, '.host-install-command').hidden = false;
  }
  // Edit's policy belongs to the entered address/key, never the old saved address.
  function wireHostPolicy(sheet, machine, fields) {
    if (!machine) return;
    const block = $(sheet, '.host-policy-field'), control = $(sheet, '.host-update-policy');
    let revision = 0, timer;
    const load = async () => {
      const mine = ++revision;
      block.hidden = true;
      const target = {address:fields.address.value.trim(), key:fields.key.value.trim()};
      if (!target.address) return;
      try {
        const reply = await hostCall(target, 'status');
        if (!sheet.isConnected || mine !== revision || !reply.supported) return;
        control.value = reply.policy || 'notify'; block.hidden = false;
        text(sheet, '.host-policy-hint', 'Changes apply immediately, for everyone who connects.');
      } catch { /* Probing beside the address already reports connection errors. */ }
    };
    for (const field of [fields.address, fields.key]) field.addEventListener('input', () => {++revision; block.hidden = true; clearTimeout(timer); timer = setTimeout(load, 500);});
    control.onchange = async () => {
      const mine = revision, chosen = control.value, target = {address:fields.address.value.trim(), key:fields.key.value.trim()};
      control.disabled = true;
      try {
        const reply = await hostCall(target, chosen);
        if (mine === revision) {control.value = reply.policy; text(sheet, '.host-policy-hint', reply.policy === chosen ? 'Saved on this computer.' : 'The host kept its existing policy. Its startup settings may override this choice.');}
      } catch(e) { if(mine === revision) {await load(); text(sheet, '.host-policy-hint', String(e));} }
      finally {control.disabled = false;}
    };
    load();
  }
  return {hostInfo, wireSettings, wireHostPolicy, showHost, paint, start() {refresh(); setInterval(refresh, 2000);}};
}
