// Motion: real springs (as CSS linear() curves), FLIP for layout changes,
// and a pointer tilt for the machines.

const reduce = matchMedia("(prefers-reduced-motion: reduce)");
const linearWorks = CSS.supports("transition-timing-function", "linear(0, 1)");

export const reducedMotion = () => reduce.matches;

/** A damped spring from 0 to 1: its CSS easing, and how long it takes to settle. */
export function spring(stiffness, damping, mass = 1) {
  const dt = 1 / 240;
  let x = 0;
  let v = 0;
  let t = 0;
  const samples = [0];
  while (t < 3) {
    v += ((-stiffness * (x - 1) - damping * v) / mass) * dt;
    x += v * dt;
    t += dt;
    samples.push(x);
    if (Math.abs(x - 1) < 0.0015 && Math.abs(v) < 0.02) break;
  }
  const duration = Math.round(t * 1000);
  if (!linearWorks) {
    return { easing: "cubic-bezier(0.2, 0.9, 0.3, 1)", duration: Math.round(duration * 0.85) };
  }
  const count = Math.min(64, samples.length);
  const points = [];
  for (let i = 0; i < count; i++) {
    const sample = samples[Math.round((i * (samples.length - 1)) / (count - 1))];
    points.push(Number(sample.toFixed(4)));
  }
  points[points.length - 1] = 1;
  return { easing: `linear(${points.join(", ")})`, duration };
}

export const springs = {
  /** Controls: presses, switches. */
  snappy: spring(430, 38),
  /** Things that pop in: sheets, menus, badges. A little overshoot. */
  bouncy: spring(320, 21),
  /** Big, calm moves: layout, flights. */
  gentle: spring(150, 23),
  /** Zooming into a machine's screen. */
  zoom: spring(125, 21),
};

for (const [name, curve] of Object.entries(springs)) {
  document.documentElement.style.setProperty(`--spring-${name}`, curve.easing);
  document.documentElement.style.setProperty(`--dur-${name}`, `${curve.duration}ms`);
}

/** Animate with a spring; resolves when done (or cancelled). */
export function animate(element, keyframes, curve = springs.gentle, options = {}) {
  const animation = element.animate(keyframes, {
    duration: reducedMotion() ? 1 : curve.duration,
    easing: curve.easing,
    ...options,
  });
  return animation.finished.catch(() => {});
}

export const wait = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

/** Where elements are now, for `flip` after the DOM changes. */
export function snapshot(elements) {
  const rects = new Map();
  for (const element of elements) rects.set(element, element.getBoundingClientRect());
  return rects;
}

/** Glide elements from their snapshot positions to where they are now. */
export function flip(rects, curve = springs.gentle) {
  for (const [element, before] of rects) {
    if (!element.isConnected) continue;
    const after = element.getBoundingClientRect();
    const dx = before.left - after.left;
    const dy = before.top - after.top;
    if (Math.abs(dx) < 0.5 && Math.abs(dy) < 0.5) continue;
    animate(element, [{ transform: `translate(${dx}px, ${dy}px)` }, { transform: "none" }], curve);
  }
}

/** Tilt `target` toward the pointer while it's over `surface`, and set
 *  --mx/--my (pointer position) for highlights. */
export function tilt(target, surface = target, max = 6) {
  let current = { x: 0, y: 0 };
  let goal = { x: 0, y: 0 };
  let frame = 0;
  const step = () => {
    current = {
      x: current.x + (goal.x - current.x) * 0.15,
      y: current.y + (goal.y - current.y) * 0.15,
    };
    target.style.setProperty("--rx", `${current.x.toFixed(3)}deg`);
    target.style.setProperty("--ry", `${current.y.toFixed(3)}deg`);
    const moving = Math.abs(goal.x - current.x) + Math.abs(goal.y - current.y) > 0.01;
    frame = moving ? requestAnimationFrame(step) : 0;
  };
  const kick = () => {
    if (!frame) frame = requestAnimationFrame(step);
  };
  surface.addEventListener("pointermove", (event) => {
    const rect = surface.getBoundingClientRect();
    const px = (event.clientX - rect.left) / rect.width;
    const py = (event.clientY - rect.top) / rect.height;
    target.style.setProperty("--mx", `${(px * 100).toFixed(1)}%`);
    target.style.setProperty("--my", `${(py * 100).toFixed(1)}%`);
    if (reducedMotion()) return;
    goal = { x: (0.5 - py) * max, y: (px - 0.5) * max * 1.4 };
    kick();
  });
  surface.addEventListener("pointerleave", () => {
    goal = { x: 0, y: 0 };
    kick();
  });
}
