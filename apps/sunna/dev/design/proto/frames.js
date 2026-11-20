// Screen illustrations adapted from dev/directions.html. No remote assets.
const photos = (n) => Array.from({ length: n }, (_, i) => `<i class="photo p${i % 4}"><b></b></i>`).join("");
const dock = () =>
  ["▥", "▧", "▤", "▦", "♪", "◷", "▣", "✉", "▱", "⌘"]
    .map((x, i) => `<i class="dockicon di${i % 4}">${x}</i>`)
    .join("");
const esc = (s) =>
  String(s ?? "")
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
const rust = (src) =>
  esc(src).replace(
    /(\/\/.*$)|("[^"\n]*")|\b(impl|for|fn|let|mut|self|pub|use|struct|if|return|Ok|Err)\b|\b([A-Z][A-Za-z0-9]*)\b|\b(\d[\d_]*)\b/gm,
    (m, c, s, k, t) => `<span class="${c ? "c" : s ? "s" : k ? "k" : t ? "ty" : "n"}">${m}</span>`,
  );
const CODE = `use std::time::{Duration, Instant};

/// Grabs the root window through MIT-SHM.
pub struct ScreenSource {
    conn: RustConnection,
    segment: Segment,
    width: u32,
    height: u32,
    next_deadline: Instant,
}

impl FrameSource for ScreenSource {
    fn next_frame(&mut self) -> Result<VideoFrame> {
        let now = Instant::now();
        if now < self.next_deadline {
            std::thread::sleep(self.next_deadline - now);
        }
        self.next_deadline += Duration::from_micros(16_667);

        // Unchanged frames are skipped before encoding.
        let image = self.grab()?;
        if !self.damage.changed(&image) {
            return Err(NoPicture.into());
        }
        Ok(VideoFrame::bgra(image, self.width, self.height))
    }
}`;
const grads = [
  "linear-gradient(135deg,#ff9a56,#ff5f6d)",
  "linear-gradient(135deg,#5ee7df,#3a7bd5)",
  "linear-gradient(135deg,#f6d365,#fda085)",
  "linear-gradient(135deg,#a18cd1,#5b4fcf)",
  "linear-gradient(135deg,#84fab0,#2f9e77)",
  "linear-gradient(135deg,#fbc2eb,#e0568f)",
  "linear-gradient(135deg,#4facfe,#1d4ed8)",
  "linear-gradient(135deg,#e2e2e6,#9c9ca6)",
  "linear-gradient(135deg,#ffd86f,#fc6262)",
  "linear-gradient(135deg,#43e97b,#11998e)",
  "linear-gradient(135deg,#30cfd0,#330867)",
  "linear-gradient(135deg,#f093fb,#f5576c)",
];
const tiles = (n, from = 0) =>
  Array.from({ length: n }, (_, i) => `<i style="--g:${grads[(i + from) % grads.length]}"></i>`).join("");
const cpu = [62, 18, 87, 34, 9, 71, 26, 44];
const SCREEN = {
  arch: () => `<div class="d arch">
    <div class="bar"><i class="ws on"></i><i class="ws"></i><i class="ws"></i><i class="ws"></i><span class="clk">Tue 17 Nov  20:14</span></div>
    <div class="t f" style="left:.8cqw;top:3.4cqw;width:57.6cqw;height:52cqw"><div class="tab">crates/capture/src/linux.rs</div><pre>${rust(CODE)}</pre></div>
    <div class="t" style="left:59.2cqw;top:3.4cqw;width:40cqw;height:27cqw"><pre><span class="p">~/projects/sunna</span> <span class="c">main</span>
<span class="g">❯</span> cargo test --release -p sunna-capture
<span class="g">   Compiling</span> sunna-capture v0.0.1
<span class="g">    Finished</span> \`release\` profile in 41.2s
<span class="g">     Running</span> unittests src/lib.rs

running 25 tests
test tiles::changed_squares ... <span class="g">ok</span>
test tiles::unchanged_is_empty ... <span class="g">ok</span>
test cursor::shape_roundtrip ... <span class="g">ok</span>
test result: <span class="g">ok</span>. 25 passed; 0 failed

<span class="g">❯</span> <span class="caret"></span></pre></div>
    <div class="t mon" style="left:59.2cqw;top:31.2cqw;width:40cqw;height:24.2cqw">
      ${cpu.map((v, i) => `<div class="mr"><b>${i}</b><i style="--v:${v}%;--c:${v > 80 ? "#f2c879" : "#58d68d"}"></i><u>${v}%</u></div>`).join("")}
      <div class="mr"><b>Mem</b><i style="--v:41%;--c:#5fd4ef"></i><u>6.2G</u></div>
      <div class="mr"><b>GPU</b><i style="--v:23%;--c:#c79bf2"></i><u>23%</u></div>
    </div></div>`,
  mac: () => `<div class="d mac">
    <div class="mb"><b>Notes</b><span>File</span><span>Edit</span><span>Format</span><span>View</span><span>Window</span><span>Help</span><span class="r">Tue 17 Nov  8:14 PM</span></div>
    <div class="w doc" style="left:7cqw;top:6cqw;width:45cqw;height:46cqw"><div class="hd"><i></i><i></i><i></i><span>Lisbon in March</span></div>
      <div class="bd"><h4>Lisbon in March</h4><p>Four days, flying in on the Thursday. Staying in Alfama, a short walk from the river.</p><div class="ph"></div>
      <ul><li>Tram 28 early, before the queues</li><li>Belém on Friday: tower, then pastries</li><li>Day trip to Sintra if it's clear</li><li>Book the fado place for Saturday</li></ul></div></div>
    <div class="w pho" style="left:45cqw;top:15cqw;width:48cqw;height:38cqw"><div class="hd"><i></i><i></i><i></i><span>Photos</span></div><div class="gr">${photos(16)}</div></div>
    <div class="dock">${dock()}</div></div>`,
  studio: () => `<div class="d gn">
    <div class="gb"><span>Activities</span><span>17 Nov  20:14</span><span>en</span></div>
    <div class="app">
      <div class="lp">${["Sun", "Sky", "Far ridge", "Near ridge", "Water", "Grade"].map((n, i) => `<div class="ly${i === 2 ? " on" : ""}"><i style="--g:${grads[(i * 2 + 1) % grads.length]}"></i>${n}</div>`).join("")}</div>
      <div class="cv"><div class="art"><div class="sun"></div><div class="mt m1"></div><div class="mt m2"></div><div class="mt m3"></div><div class="gl"></div></div></div>
      <div class="rp">${[
        ["Exposure", 58],
        ["Contrast", 36],
        ["Warmth", 74],
        ["Tint", 47],
        ["Saturation", 66],
        ["Grain", 12],
      ]
        .map(([n, v]) => `<div class="sl"><span>${n}</span><i style="--v:${v}%"></i></div>`)
        .join("")}</div>
      <div class="tm">
        <div class="tr"><i style="--l:2%;--w:27%;--g:#2f9e8f"></i><i style="--l:30%;--w:38%;--g:#3a7bd5"></i><i style="--l:69%;--w:22%;--g:#2f9e8f"></i></div>
        <div class="tr"><i style="--l:8%;--w:44%;--g:#c2772e"></i><i style="--l:55%;--w:30%;--g:#b24a6c"></i></div>
        <div class="tr"><i style="--l:2%;--w:92%;--g:#5b4fcf"></i></div><div class="ph"></div>
      </div>
    </div></div>`,
  ubuntu: () => `<div class="d ub">
    <div class="ub-b">17 Nov  14:44</div>
    <div class="ub-d">${tiles(7, 5)}</div>
    <div class="w te" style="left:9cqw;top:6cqw;width:50cqw;height:30cqw"><div class="hd">sunny@ubuntu: ~</div><pre><span class="g">sunny@ubuntu</span>:<span class="p">~</span>$ sunna-host status
Sunna host   running since Mon 09:12
Listening    0.0.0.0:48800
Desktop      :40  1710x1112
Sound        on
Viewer       none

<span class="g">sunny@ubuntu</span>:<span class="p">~</span>$ <span class="caret"></span></pre></div>
    <div class="w fi" style="left:46cqw;top:24cqw;width:46cqw;height:27cqw"><div class="hd">Home</div><div class="gr">${["Desktop", "Documents", "Downloads", "Music", "Pictures", "projects", "sunna-logs", "Videos"].map((n) => `<div><i></i>${n}</div>`).join("")}</div></div></div>`,
  gaming: () => `<div class="d w11">
    ${[-52, -26, 0, 26, 52].map((r) => `<i class="pt" style="--r:${r}deg"></i>`).join("")}
    <div class="w"><div class="sb"><b>Library</b><span>Home</span><span>Installed</span><span>Favourites</span><span>Recent</span><span>Downloads</span></div>
      <div class="mn"><h4>Recent</h4><div class="cs">${tiles(10, 1)}</div></div></div>
    <div class="tk">${tiles(8, 6)}<span>20:14</span></div></div>`,
};
const LOCK = `<div class="lock"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><rect x="5" y="10.5" width="14" height="9.5" rx="2.2"/><path d="M8.2 10.5V8a3.8 3.8 0 0 1 7.6 0v2.5"/></svg></div>`;
