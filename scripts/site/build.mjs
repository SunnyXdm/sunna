// Build with npm ci --prefix scripts/site && node scripts/site/build.mjs.
// Requires ImageMagick 7. Serve dist/site as a static directory.
import { execFileSync } from 'node:child_process';
import { cpSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import MarkdownIt from 'markdown-it';
import { rgbaToThumbHash, thumbHashToRGBA } from 'thumbhash';

const root = fileURLToPath(new URL('../../', import.meta.url));
const docs = join(root, 'docs');
const out = process.env.SUNNA_SITE_OUT || join(root, 'dist/site');
const guides = join(out, 'guides');
mkdirSync(guides, { recursive: true });
// Copy exactly the homepage media plus the images used in the guides.
const homepage = readFileSync(join(docs, 'index.html'), 'utf8');
const media = [...homepage.matchAll(/(?:src|poster)="\/(images\/[^"]+)"/g)].map(match => match[1]);
for (const entry of new Set(['index.html', 'style.css', 'images/icon.png', 'images/gallery.webp', 'images/devices.webp', 'images/wall.webp', ...media])) {
  cpSync(join(docs, entry), join(out, entry), { recursive: true });
}

const pictures = new Map();
const rules = [];
for (const name of ['gallery.webp', 'devices.webp', 'wall.webp']) {
  const file = join(docs, 'images', name);
  const size = execFileSync('magick', ['identify', '-format', '%w %h', file], { encoding: 'utf8' });
  const [width, height] = size.split(' ').map(Number);
  pictures.set(name, { width, height });
  const w = Math.round(width * 100 / Math.max(width, height));
  const h = Math.round(height * 100 / Math.max(width, height));
  const pixels = execFileSync('magick', [file, '-resize', w + 'x' + h + '!', '-depth', '8', 'rgba:-']);
  const hash = rgbaToThumbHash(w, h, pixels);
  const preview = thumbHashToRGBA(hash);
  const webp = execFileSync('magick', ['-size', preview.w + 'x' + preview.h, '-depth', '8', 'rgba:-', 'webp:-'], { input: preview.rgba });
  rules.push('/* ThumbHash ' + Buffer.from(hash).toString('base64') + ' */\n' +
    'img[src$="/' + name + '"] {background: center / 100% 100% url(data:image/webp;base64,' + webp.toString('base64') + ')}');
}
writeFileSync(join(guides, 'previews.css'), rules.join('\n') + '\n');

const pages = {
  'get-started': 'Get started', 'host-linux': 'Share a Linux computer', 'host-mac': 'Share a Mac',
  'app-linux': 'Linux app', 'app-mac': 'Mac app', 'app-android': 'Android app',
  using: 'Using Sunna', troubleshooting: 'Troubleshooting', security: 'Security and privacy',
  building: 'Building from source', 'update-verification': 'Update verification',
};
const address = slug => slug === 'get-started' ? '/guides/' : '/guides/' + slug + '.html';
const markdown = new MarkdownIt().enable('table');
const render = markdown.renderer.rules;
render.link_open = (tokens, i, options, env, self) => {
  const token = tokens[i];
  const href = token.attrGet('href');
  if (href.startsWith('../')) {
    token.attrSet('href', 'https://github.com/SunnyXdm/sunna/blob/main/' + href.slice(3));
  } else if (href.startsWith('dev/')) {
    token.attrSet('href', 'https://github.com/SunnyXdm/sunna/blob/main/docs/' + href);
  } else if (href.endsWith('.md')) {
    const slug = href.slice(0, -3);
    if (!Object.hasOwn(pages, slug)) throw new Error('Missing guide: ' + href);
    token.attrSet('href', address(slug));
  }
  return self.renderToken(tokens, i, options);
};
const renderImage = render.image;
render.image = (tokens, i, options, env, self) => {
  const token = tokens[i];
  const src = token.attrGet('src');
  if (src.startsWith('images/')) {
    const { width, height } = pictures.get(src.slice(7));
    token.attrSet('src', '/' + src);
    token.attrSet('width', String(width));
    token.attrSet('height', String(height));
    token.attrSet('loading', 'lazy');
  }
  return renderImage(tokens, i, options, env, self);
};
render.table_open = () => '<div class="guide-table"><table>';
render.table_close = () => '</table></div>\n';

for (const [slug, title] of Object.entries(pages)) {
  const source = readFileSync(join(docs, slug + '.md'), 'utf8').replace('[← Back to the README](../README.md)', '');
  const navigation = Object.entries(pages).map(([key, label]) =>
    '<a href="' + address(key) + '"' + (slug === key ? ' aria-current="page"' : '') + '>' + label + '</a>'
  ).join('\n');
  const html = `<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>${title} · Sunna guides</title><link rel="icon" href="/images/icon.png"><link rel="stylesheet" href="/style.css"><link rel="stylesheet" href="/guides/previews.css"></head>
<body><a class="skip" href="#content">Skip to guide</a>
<header class="nav"><a class="brand" href="/"><img src="/images/icon.png" width="30" height="30" alt="">Sunna</a><a href="/guides/">Guides</a></header>
<main class="guide-layout"><nav aria-label="Guides">${navigation}</nav><article id="content">${markdown.render(source)}</article></main>
</body></html>\n`;
  writeFileSync(join(guides, slug === 'get-started' ? 'index.html' : slug + '.html'), html);
}

cpSync(join(root, 'site/fonts'), join(out, 'fonts'), { recursive: true });
