// A small Chrome DevTools driver for looking at the launcher UI: launch
// headless Chrome, click/type/hover, screenshot, and record screencasts to
// MP4 for checking animations. Node 24+ (built-in WebSocket), google-chrome
// and ffmpeg. With the mock (mock.js) serving:
//
//   import { launch } from "./apps/sunna/dev/cdp.mjs";
//   const { page, close } = await launch();
//   await page.goto("http://127.0.0.1:8766/ui/?time=19:30");
//   await page.shot("/tmp/home.png");
import { spawn, execFileSync } from "node:child_process";
import { mkdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

export async function launch({ width = 980, height = 660, scale = 2, port = 9333 } = {}) {
  rmSync(`${tmpdir()}/sunna-cdp-profile`, { recursive: true, force: true });
  const chrome = spawn("google-chrome", [
    "--headless=new", `--remote-debugging-port=${port}`, `--window-size=${width},${height}`,
    `--force-device-scale-factor=${scale}`, "--hide-scrollbars", "--no-first-run",
    "--no-default-browser-check", `--user-data-dir=${tmpdir()}/sunna-cdp-profile`, "--disable-gpu", "about:blank",
  ], { stdio: "ignore" });
  let targets = [];
  for (let i = 0; i < 50; i++) {
    try {
      targets = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
      if (targets.some((t) => t.type === "page")) break;
    } catch {}
    await sleep(100);
  }
  const target = targets.find((t) => t.type === "page");
  const ws = new WebSocket(target.webSocketDebuggerUrl);
  await new Promise((resolve) => (ws.onopen = resolve));
  let id = 0;
  const pending = new Map();
  const handlers = new Map();
  ws.onmessage = (event) => {
    const msg = JSON.parse(event.data);
    if (msg.id && pending.has(msg.id)) {
      const { resolve, reject } = pending.get(msg.id);
      pending.delete(msg.id);
      msg.error ? reject(new Error(JSON.stringify(msg.error))) : resolve(msg.result);
    } else if (msg.method) {
      for (const handler of handlers.get(msg.method) ?? []) handler(msg.params);
    }
  };
  const send = (method, params = {}) =>
    new Promise((resolve, reject) => {
      const n = ++id;
      pending.set(n, { resolve, reject });
      ws.send(JSON.stringify({ id: n, method, params }));
    });
  const on = (method, handler) => handlers.set(method, [...(handlers.get(method) ?? []), handler]);
  await send("Page.enable");
  await send("Runtime.enable");
  const logs = [];
  on("Runtime.consoleAPICalled", (p) => logs.push(`${p.type}: ${p.args.map((a) => a.value ?? a.description).join(" ")}`));
  on("Runtime.exceptionThrown", (p) => logs.push(`EXCEPTION: ${p.exceptionDetails.exception?.description ?? p.exceptionDetails.text}`));
  await send("Emulation.setDeviceMetricsOverride", { width, height, deviceScaleFactor: scale, mobile: false });

  const page = {
    logs,
    send,
    async goto(url) {
      const loaded = new Promise((resolve) => on("Page.loadEventFired", resolve));
      await send("Page.navigate", { url });
      await loaded;
    },
    async eval(expression) {
      const result = await send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true });
      if (result.exceptionDetails) throw new Error(result.exceptionDetails.exception?.description ?? "eval failed");
      return result.result.value;
    },
    async center(selector) {
      const rect = await page.eval(`(() => { const r = document.querySelector(${JSON.stringify(selector)})?.getBoundingClientRect(); return r && { x: r.left + r.width / 2, y: r.top + r.height / 2 }; })()`);
      if (!rect) throw new Error(`no element ${selector}`);
      return rect;
    },
    async move(x, y) {
      await send("Input.dispatchMouseEvent", { type: "mouseMoved", x, y });
    },
    async hover(selector, dx = 0, dy = 0) {
      const { x, y } = await page.center(selector);
      await page.move(x + dx, y + dy);
    },
    async click(selector, { button = "left" } = {}) {
      const { x, y } = await page.center(selector);
      await page.move(x, y);
      await send("Input.dispatchMouseEvent", { type: "mousePressed", x, y, button, clickCount: 1 });
      await send("Input.dispatchMouseEvent", { type: "mouseReleased", x, y, button, clickCount: 1 });
    },
    async type(text) {
      await send("Input.insertText", { text });
    },
    async press(key, modifiers = 0) {
      const codes = { Escape: 27, Enter: 13, Tab: 9, ArrowRight: 39, ArrowLeft: 37, ArrowDown: 40, ArrowUp: 38, Backspace: 8 };
      const code = codes[key] ?? key.toUpperCase().charCodeAt(0);
      await send("Input.dispatchKeyEvent", { type: "rawKeyDown", key, windowsVirtualKeyCode: code, modifiers });
      await send("Input.dispatchKeyEvent", { type: "keyUp", key, windowsVirtualKeyCode: code, modifiers });
    },
    async shot(path) {
      const { data } = await send("Page.captureScreenshot", { format: "png" });
      writeFileSync(path, Buffer.from(data, "base64"));
    },
    /** Record while `fn` runs; writes an MP4 at `path` (constant 30 fps). */
    async record(path, fn, { scale: videoScale = 1 } = {}) {
      const dir = `${tmpdir()}/sunna-cdp-frames-${Date.now()}`;
      mkdirSync(dir, { recursive: true });
      const frames = [];
      on("Page.screencastFrame", async (p) => {
        frames.push({ data: p.data, t: p.metadata.timestamp });
        send("Page.screencastFrameAck", { sessionId: p.sessionId }).catch(() => {});
      });
      await send("Page.startScreencast", { format: "jpeg", quality: 88, everyNthFrame: 1, maxWidth: width * videoScale * 2, maxHeight: height * videoScale * 2 });
      await fn();
      await sleep(200);
      await send("Page.stopScreencast");
      handlers.set("Page.screencastFrame", []);
      // Hold each frame until the next one: a concat list with durations.
      let list = "";
      frames.forEach((frame, i) => {
        const file = `${dir}/${String(i).padStart(5, "0")}.jpg`;
        writeFileSync(file, Buffer.from(frame.data, "base64"));
        const next = frames[i + 1]?.t ?? frame.t + 0.3;
        list += `file '${file}'\nduration ${Math.max(0.001, next - frame.t).toFixed(4)}\n`;
      });
      if (frames.length) list += `file '${dir}/${String(frames.length - 1).padStart(5, "0")}.jpg'\n`;
      writeFileSync(`${dir}/list.txt`, list);
      execFileSync("ffmpeg", ["-loglevel", "error", "-y", "-f", "concat", "-safe", "0", "-i", `${dir}/list.txt`,
        "-vf", `fps=30,scale=${width * videoScale}:-2:flags=lanczos,format=yuv420p`, "-c:v", "libx264", "-crf", "20", "-movflags", "+faststart", path]);
      rmSync(dir, { recursive: true, force: true });
      return frames.length;
    },
    sleep,
  };
  return {
    page,
    close: () => {
      try { ws.close(); } catch {}
      chrome.kill();
    },
  };
}
