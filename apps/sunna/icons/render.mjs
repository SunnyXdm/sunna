// Renders icon.svg to the app's PNG icons and a macOS icon.icns, each size
// drawn from the vector (not resampled), with a transparent background.
//
//   python3 apps/sunna/icons/make_svg.py && node apps/sunna/icons/render.mjs
import { writeFileSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { launch } from "../dev/cdp.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const sizes = [16, 32, 64, 128, 256, 512, 1024];

const { page, close } = await launch({ width: 1024, height: 1024, scale: 1 });
const png = {};
try {
  await page.send("Emulation.setDefaultBackgroundColorOverride", { color: { r: 0, g: 0, b: 0, a: 0 } });
  await page.goto(`file://${join(here, "icon.svg")}`);
  for (const size of sizes) {
    await page.eval(`(() => { const svg = document.documentElement; svg.setAttribute("width", ${size}); svg.setAttribute("height", ${size}); })()`);
    await page.sleep(150);
    const { data } = await page.send("Page.captureScreenshot", {
      format: "png",
      clip: { x: 0, y: 0, width: size, height: size, scale: 1 },
    });
    png[size] = Buffer.from(data, "base64");
  }
} finally {
  close();
}

writeFileSync(join(here, "icon.png"), png[1024]);
writeFileSync(join(here, "32x32.png"), png[32]);
writeFileSync(join(here, "128x128.png"), png[128]);
writeFileSync(join(here, "128x128@2x.png"), png[256]);

// icns: "icns", total length, then (type, length incl. header, PNG data).
const entries = [
  ["icp4", 16], ["icp5", 32], ["icp6", 64], ["ic07", 128], ["ic08", 256], ["ic09", 512],
  ["ic10", 1024], ["ic11", 32], ["ic12", 64], ["ic13", 256], ["ic14", 512],
];
const chunks = entries.map(([type, size]) => {
  const header = Buffer.alloc(8);
  header.write(type, 0, "ascii");
  header.writeUInt32BE(png[size].length + 8, 4);
  return Buffer.concat([header, png[size]]);
});
const body = Buffer.concat(chunks);
const head = Buffer.alloc(8);
head.write("icns", 0, "ascii");
head.writeUInt32BE(body.length + 8, 4);
writeFileSync(join(here, "icon.icns"), Buffer.concat([head, body]));
console.log("wrote icon.png, 32x32.png, 128x128.png, 128x128@2x.png, icon.icns");
