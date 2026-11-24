// Draws the README's banner (light and dark) and its download buttons, with Ipse's own
// fonts and colours (docs/design/charter.md). Run from ui/, after npm ci:
//   node ../docs/images/source/readme-images.mjs
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { createRequire } from 'node:module';

// Playwright comes from ui/, where this runs.
const { chromium } = createRequire(join(process.cwd(), 'package.json'))('playwright');

const here = dirname(fileURLToPath(import.meta.url));
const out = join(here, '..');
const font = (path) => `data:font/woff2;base64,${readFileSync(join('node_modules', path)).toString('base64')}`;
const fonts = `
@font-face { font-family: Display; font-weight: 200 800; src: url(${font('@fontsource-variable/bricolage-grotesque/files/bricolage-grotesque-latin-wght-normal.woff2')}); }
@font-face { font-family: Text; font-weight: 200 800; src: url(${font('@fontsource-variable/atkinson-hyperlegible-next/files/atkinson-hyperlegible-next-latin-wght-normal.woff2')}); }
* { margin: 0; box-sizing: border-box; }`;

const themes = {
	light: { ground: '#fbfaf8', text: '#231d1e', muted: '#6e6466', thread: '#9b2c3a', faint: '#e8c9cd' },
	dark: { ground: '#171314', text: '#f0eaea', muted: '#ada2a3', thread: '#cf6874', faint: '#5a3238' }
};

const mark = (fill) =>
	`<svg viewBox="0 0 40 40" width="84" height="84"><rect width="40" height="40" rx="11" fill="${fill}"/><path d="M16 33V19.5a6.5 6.5 0 1 1 6.5 6.5" fill="none" stroke="#fff" stroke-width="3.8" stroke-linecap="round"/><circle cx="16" cy="9" r="3" fill="#fff"/></svg>`;

const banner = (t) => `<!doctype html><html><head><style>${fonts}
body { width: 1280px; height: 360px; background: ${t.ground}; position: relative; overflow: hidden; }
.thread { position: absolute; inset: 0; }
.content { position: absolute; inset: 0; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 22px; padding-bottom: 24px; }
.name { display: flex; align-items: center; gap: 22px; }
.name span { font: 700 104px/1 Display; color: ${t.text}; letter-spacing: -0.04em; margin-top: -14px; }
p { font: 500 27px/1.35 Text; color: ${t.muted}; text-align: center; }
p strong { color: ${t.text}; font-weight: 700; }
</style></head><body>
<svg class="thread" viewBox="0 0 1280 360" preserveAspectRatio="none">
  <path d="M-20 338 C 220 318, 380 352, 640 330 S 1060 312, 1300 336" fill="none" stroke="${t.thread}" stroke-width="2.5" opacity="0.55"/>
  <path d="M-20 348 C 260 334, 420 360, 660 342 S 1080 326, 1300 348" fill="none" stroke="${t.faint}" stroke-width="1.5"/>
</svg>
<div class="content">
  <div class="name">${mark(t.thread)}<span>ipse</span></div>
  <p><strong>The memory of your whole life, as files you own.</strong><br>Your AI reads what you allow. Nothing is written until you say yes.</p>
</div></body></html>`;

const button = (label) => `<!doctype html><html><head><style>${fonts}
body { background: transparent; display: inline-block; }
a { display: inline-flex; align-items: center; height: 56px; padding: 0 28px; border-radius: 999px; background: #9b2c3a; color: #fff; font: 700 21px/1 Text; white-space: nowrap; }
</style></head><body><a>${label}</a></body></html>`;

const browser = await chromium.launch();
const page = await browser.newPage({ deviceScaleFactor: 2 });
for (const [name, theme] of Object.entries(themes)) {
	await page.setViewportSize({ width: 1280, height: 360 });
	await page.setContent(banner(theme));
	await page.evaluate(() => document.fonts.ready);
	await page.screenshot({ path: join(out, `banner-${name}.png`) });
}
for (const [file, label] of [['download-macos', 'Download for macOS'], ['download-windows', 'Download for Windows'], ['download-linux', 'Download for Linux']]) {
	await page.setViewportSize({ width: 400, height: 56 });
	await page.setContent(button(label));
	await page.evaluate(() => document.fonts.ready);
	await page.locator('a').screenshot({ path: join(out, `${file}.png`), omitBackground: true });
}
await browser.close();
