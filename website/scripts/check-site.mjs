import { readFile, access } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { resolve, dirname, sep } from 'node:path';
import assert from 'node:assert/strict';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const dist = resolve(root, 'out');
const html = await readFile(resolve(dist, 'index.html'), 'utf8');
const manifest = JSON.parse(await readFile(resolve(root, '.openai/hosting.json'), 'utf8'));
const dependencies = JSON.parse(await readFile(resolve(root, 'package.json'), 'utf8')).dependencies;
assert.equal(manifest.static.directory, 'out', 'Hosting must use the Next.js export');
for (const dependency of ['next', 'react', 'tailwindcss', 'radix-ui']) {
  assert.ok(dependencies[dependency], `Missing framework dependency: ${dependency}`);
}
assert.match(html, /\/_next\/static\//, 'The published page must include Next.js assets');
const ids = [...html.matchAll(/\bid="([^"]+)"/g)].map(match => match[1]);
assert.equal(ids.length, new Set(ids).size, 'HTML IDs must be unique');
assert.equal([...html.matchAll(/<h1\b/g)].length, 1, 'There must be one primary heading');
assert.match(html, /<html lang="en">/);
assert.match(html, /name="viewport"/);
assert.match(html, /name="description"/);
assert.match(html, /rel="icon"/);

let checkedAssets = 0;
for (const [, attribute, value] of html.matchAll(/\b(href|src)="([^"]+)"/g)) {
  if (value.startsWith('#')) {
    assert.ok(value === '#' || ids.includes(value.slice(1)), `Broken section link: ${value}`);
  } else if (!/^(?:https:|data:)/.test(value)) {
    const path = resolve(dist, value.split('?')[0].replace(/^\//, ''));
    assert.ok(path.startsWith(dist + sep), `Asset leaves the public directory: ${value}`);
    await access(path);
    checkedAssets += 1;
  }
  if (attribute === 'href') assert.ok(!value.startsWith('javascript:'), 'Unsafe link');
}

const installer = 'irm https://raw.githubusercontent.com/blocksdevpro/vstretch/main/install.ps1 | iex';
// Collapsible content mounts on demand; verify its shared source independently.
const links = await readFile(resolve(root, 'lib/site.ts'), 'utf8');
assert.ok(links.includes(installer), 'The install command must match the documented installer');
const assetLinks = [...html.matchAll(/href="(https:\/\/github\.com\/blocksdevpro\/vstretch\/releases\/(?:download\/[^/]+|latest\/download)\/vstretch\.exe)"/g)].map(match => match[1]);
assert.equal(assetLinks.length, 2, 'Both download buttons must point to an executable');
assert.equal(assetLinks[0], assetLinks[1], 'Download buttons must use the same release');
const tag = /releases\/download\/([^/]+)\//.exec(assetLinks[0])?.[1];
if (tag) {
  assert.ok(html.includes(`${tag} · `), 'Download version must match the linked asset');
  assert.ok(html.includes(`/releases/tag/${tag}`), 'Checksum link must use the download release');
  assert.ok(html.includes(`/tree/${tag}`), 'Source link must use the download release');
}
assert.match(html, /Windows 10\/11/);
assert.match(html, /Try the preview/);
assert.doesNotMatch(html, /vStretch|VStretch/, 'Product naming must use vstretch');
assert.match(html, /Windows protected your PC/);
assert.match(html, /Get-FileHash/);
assert.match(html, /Vanguard \/ VAC/);
const page = await readFile(resolve(root, 'app/page.tsx'), 'utf8');
// FAQ answers mount when opened; verify their copy in the page source.
assert.match(page, /does not read or write game memory/);
assert.match(page, /cannot guarantee/);
assert.match(html, /Not affiliated with Riot Games or Valve/);
assert.doesNotMatch(html, /nextjs-portal|data-nextjs-dialog|__nextjs_original-stack-frame/, 'Production HTML must not include dev-tools overlays');

const faqButtons = [...html.matchAll(/<button\b[^>]*data-slot="accordion-trigger"[^>]*>/g)].map(match => match[0]);
assert.equal(faqButtons.length, 5, 'FAQ topics must remain discoverable');
assert.ok(faqButtons.every(button => button.includes('aria-expanded="false"')), 'FAQ answers should start collapsed');
const safetyDetails = /<details\b[^>]*id="download-checks"[^>]*>/.exec(html)?.[0];
assert.ok(safetyDetails && !/\bopen(?:\s|=|>)/.test(safetyDetails), 'Download instructions should start collapsed');

// Keep the initial view scannable; exclude scripts, illustrations, screen-reader
// announcements, and the contents of closed details from the visible word count.
const visibleMarkup = html.replace(/<(script|style|svg)\b[^>]*>[\s\S]*?<\/\1>/g, '')
  .replace(/<details\b[^>]*>([\s\S]*?)<\/details>/g, (_, content) => /<summary\b[^>]*>[\s\S]*?<\/summary>/.exec(content)?.[0] ?? '')
  .replace(/<p\b[^>]*class="[^"]*sr-only[^"]*"[^>]*>[\s\S]*?<\/p>/g, '')
  .replace(/<!--[\s\S]*?-->/g, '')
  .replace(/<[^>]+>/g, ' ');
const visibleWordCount = visibleMarkup.trim().split(/\s+/).length;
assert.ok(visibleWordCount <= 300, `Initial page copy is too dense: ${visibleWordCount} words (budget: 300)`);

// Verify the actual theme values against the backgrounds used by small text.
// This covers text contrast, not a full browser accessibility audit.
const css = await readFile(resolve(root, 'app/globals.css'), 'utf8');
const color = name => {
  const value = new RegExp(`--${name}: (#[a-f0-9]{6});`).exec(css)?.[1];
  assert.ok(value, `Missing theme color: ${name}`);
  return value;
};
function luminance(hex) {
  const rgb = hex.slice(1).match(/../g).map(part => parseInt(part, 16) / 255)
    .map(channel => channel <= 0.04045 ? channel / 12.92 : ((channel + 0.055) / 1.055) ** 2.4);
  return rgb[0] * 0.2126 + rgb[1] * 0.7152 + rgb[2] * 0.0722;
}
const pairs = [
  ...[color('background'), color('card'), color('secondary'), '#f5f4ef', '#f7ece4', '#fffbf7', '#f6f5ef', '#f7f9f5'].map(background => [color('muted-foreground'), background]),
  [color('foreground'), color('background')],
  ['#ffffff', color('primary')],
  ['#ffffff', '#aa4633'],
  [color('primary'), color('background')],
  [color('primary'), '#fffbf7'],
  ['#aa4633', '#f7ece4'],
  ['#aa4633', '#fffbf7'],
  ['#786453', '#f7ece4'],
  ['#765f4c', '#f7ece4'],
  ['#765f4c', '#fffbf7'],
  ['#a64432', '#f9e9e1'],
  ['#496751', '#fcfcfa'],
  ['#596050', '#fcfcfa'],
];
let minimumContrast = Infinity;
for (const [foreground, background] of pairs) {
  const values = [luminance(foreground), luminance(background)].sort((a, b) => b - a);
  const ratio = (values[0] + 0.05) / (values[1] + 0.05);
  assert.ok(ratio >= 4.5, `WCAG AA text contrast failed: ${foreground} on ${background} = ${ratio.toFixed(2)}:1`);
  minimumContrast = Math.min(minimumContrast, ratio);
}
console.log(`Site checks passed: ${visibleWordCount} initial-view words, collapsed details/FAQ, ${ids.length} unique IDs, ${checkedAssets} local assets, matching download/source links, safety copy, naming, no dev overlay, and ${pairs.length} WCAG AA text pairs (minimum ${minimumContrast.toFixed(2)}:1).`);
