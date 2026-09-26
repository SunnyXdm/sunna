// The sky follows the local time: deep blue at night with stars, pink at
// dawn, warm light through the day, orange and violet at sunset.

// Minute of day → sun and haze colors (with strength), the low sky, where
// the sun sits across the window (0..1), and how many stars show.
const STOPS = [
  { at: 0, sun: [96, 110, 255], a: 0.3, haze: [40, 52, 150], h: 0.26, low: [18, 20, 52], x: 0.72, stars: 1 },
  { at: 300, sun: [124, 92, 255], a: 0.34, haze: [255, 92, 140], h: 0.16, low: [30, 20, 60], x: 0.18, stars: 0.8 },
  { at: 390, sun: [255, 138, 76], a: 0.62, haze: [255, 80, 122], h: 0.3, low: [70, 30, 60], x: 0.22, stars: 0.12 },
  { at: 540, sun: [255, 197, 107], a: 0.5, haze: [74, 168, 255], h: 0.24, low: [42, 36, 58], x: 0.36, stars: 0 },
  { at: 780, sun: [255, 214, 150], a: 0.4, haze: [70, 140, 255], h: 0.36, low: [28, 40, 78], x: 0.5, stars: 0 },
  { at: 1020, sun: [255, 179, 92], a: 0.55, haze: [255, 122, 89], h: 0.24, low: [60, 36, 50], x: 0.66, stars: 0 },
  { at: 1125, sun: [255, 106, 61], a: 0.7, haze: [255, 61, 127], h: 0.34, low: [82, 28, 52], x: 0.78, stars: 0.05 },
  { at: 1200, sun: [176, 76, 255], a: 0.46, haze: [61, 76, 255], h: 0.28, low: [42, 22, 70], x: 0.84, stars: 0.5 },
  { at: 1320, sun: [91, 108, 255], a: 0.32, haze: [31, 42, 107], h: 0.24, low: [20, 20, 55], x: 0.8, stars: 1 },
  { at: 1440, sun: [96, 110, 255], a: 0.3, haze: [40, 52, 150], h: 0.26, low: [18, 20, 52], x: 0.72, stars: 1 },
];

/** Now, or the `?time=HH:MM` override (for looking at other times of day). */
export function skyTime() {
  const override = new URLSearchParams(location.search).get("time");
  const now = new Date();
  const match = override && /^(\d{1,2}):(\d{2})$/.exec(override);
  if (match) now.setHours(Number(match[1]), Number(match[2]), 0, 0);
  return now;
}

const mix = (a, b, t) => a + (b - a) * t;
const mixRgb = (a, b, t) => a.map((value, i) => Math.round(mix(value, b[i], t))).join(" ");

function paint(date) {
  const minute = date.getHours() * 60 + date.getMinutes();
  const index = STOPS.findIndex((stop) => stop.at > minute);
  const [from, to] = [STOPS[index - 1], STOPS[index]];
  const t = (minute - from.at) / (to.at - from.at);
  const style = document.documentElement.style;
  style.setProperty("--sun", mixRgb(from.sun, to.sun, t));
  style.setProperty("--sun-a", mix(from.a, to.a, t).toFixed(3));
  style.setProperty("--haze", mixRgb(from.haze, to.haze, t));
  style.setProperty("--haze-a", mix(from.h, to.h, t).toFixed(3));
  style.setProperty("--low", mixRgb(from.low, to.low, t));
  style.setProperty("--sun-x", mix(from.x, to.x, t).toFixed(3));
  style.setProperty("--stars", mix(from.stars, to.stars, t).toFixed(3));
}

/** Scatter the stars (the same sky every launch). */
function makeStars() {
  let seed = 7;
  const random = () => {
    seed = (seed * 16807) % 2147483647;
    return seed / 2147483647;
  };
  const stars = document.getElementById("stars");
  for (let i = 0; i < 70; i++) {
    const star = document.createElement("span");
    star.className = "star";
    star.style.left = `${(random() * 100).toFixed(2)}%`;
    star.style.top = `${(random() * 100).toFixed(2)}%`;
    star.style.setProperty("--size", `${random() < 0.15 ? 2 : 1.2}px`);
    star.style.setProperty("--time", `${(2.5 + random() * 4).toFixed(2)}s`);
    star.style.setProperty("--delay", `${(-random() * 6).toFixed(2)}s`);
    stars.appendChild(star);
  }
}

export function startSky() {
  makeStars();
  paint(skyTime());
  setInterval(() => paint(skyTime()), 60_000);
}
