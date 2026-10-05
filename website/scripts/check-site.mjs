import { readFile, access } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { resolve, dirname, sep } from 'node:path';
import assert from 'node:assert/strict';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const repoRoot = resolve(root, '..');
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
    // Allow same-domain endpoint routes that resolve to generated HTML files.
    const clean = value.split('?')[0].replace(/^\//, '');
    const candidates = [resolve(dist, clean), resolve(dist, `${clean}.html`), resolve(dist, clean, 'index.html')];
    assert.ok(candidates.some((p) => p.startsWith(dist + sep)), `Asset leaves the public directory: ${value}`);
    let found = false;
    for (const candidate of candidates) {
      try { await access(candidate); found = true; break; } catch { /* try next */ }
    }
    // /download and /changelog are validated separately below; other locals must exist.
    if (!['download', 'changelog', 'install.ps1', 'install.sh'].includes(clean)) {
      assert.ok(found, `Missing local asset: ${value}`);
    }
    checkedAssets += 1;
  }
  if (attribute === 'href') assert.ok(!value.startsWith('javascript:'), 'Unsafe link');
}

const links = await readFile(resolve(root, 'lib/site.ts'), 'utf8');
assert.ok(links.includes('/install.ps1') && links.includes('siteUrl'), 'The install command must match the documented installer');
assert.ok(links.includes('irm ') && links.includes('| iex'), 'The install command must be a PowerShell one-liner');
assert.ok(links.includes('https://vstretch.blocksdev.pro'), 'Site endpoints must use the production domain');
assert.ok(links.includes('"/download"'), 'Downloads must use the same-domain endpoint');
const copyCommand = await readFile(resolve(root, 'components/copy-command.tsx'), 'utf8');
assert.ok(copyCommand.includes('installPsEndpoint') || copyCommand.includes('/install.ps1'), 'Install script link must use the website endpoint');
assert.doesNotMatch(copyCommand, /raw\.githubusercontent\.com.*install\.ps1/, 'Install script link must not point at GitHub');
const downloadButtons = [...html.matchAll(/href="\/download"/g)].map(match => match[0]);
assert.equal(downloadButtons.length, 3, 'Download buttons and footer count must use the website endpoint');
// Retrieval endpoints must exist as static files.
await access(resolve(dist, 'install.ps1'));
await access(resolve(dist, 'install.sh'));
const shippedInstaller = await readFile(resolve(dist, 'install.ps1'), 'utf8');
const rootInstaller = await readFile(resolve(repoRoot, 'install.ps1'), 'utf8');
assert.equal(shippedInstaller, rootInstaller, 'Shipped install.ps1 must match the repository installer');
const shippedSh = await readFile(resolve(dist, 'install.sh'), 'utf8');
assert.match(shippedSh, /vstretch\.blocksdev\.pro\/install\.ps1/, 'Shell installer must fetch from the website endpoint');
// /download must redirect to the pinned release asset.
const downloadHtml = await readFile(resolve(dist, 'download.html'), 'utf8');
assert.match(downloadHtml, /vstretch\.exe/, 'Download endpoint must reference the executable');
assert.match(downloadHtml, /github\.com\/blocksdevpro\/vstretch\/releases/, 'Download endpoint must resolve to the pinned release');
const releaseTag = /releases\/download\/([^/]+)\//.exec(downloadHtml)?.[1] ?? /releases\/tag\/([^"<>]+)/.exec(html)?.[1];
if (releaseTag) {
  assert.ok(html.includes(`/releases/tag/${releaseTag}`) || html.includes(`${releaseTag} · `) || downloadHtml.includes(releaseTag), 'Checksum/source links must use the download release');
}
// /changelog must exist and list releases from CHANGELOG.md.
const changelogHtml = await readFile(resolve(dist, 'changelog.html'), 'utf8');
assert.match(changelogHtml, /Changelog\./, 'Changelog page must have a heading');
assert.match(changelogHtml, /v1\.2\.0|1\.2\.0/, 'Changelog page must include release notes');
assert.match(html, /href="\/changelog"/, 'Homepage must link to the changelog');
assert.match(html, /Windows 10\/11/);
assert.match(html, /Try the preview/);
assert.doesNotMatch(html, /vStretch|VStretch/, 'Product naming must use vstretch');
// Verify section stays calm and unsigned-fear-free while SignPath is in progress.
assert.doesNotMatch(html, /Unsigned app/, 'Unsigned label must not add fear');
assert.doesNotMatch(html, /Windows protected your PC/, 'SmartScreen warning must stay soft');
assert.doesNotMatch(html, /SmartScreen/, 'SmartScreen headline must stay soft');
assert.match(html, /Verify download/);
assert.match(html, /Get-FileHash/);
assert.match(html, /Signed builds are on the way/);
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
console.log(`Site checks passed: ${visibleWordCount} initial-view words, collapsed details/FAQ, ${ids.length} unique IDs, ${checkedAssets} local assets, same-domain download/installer/changelog, calm verify copy, naming, no dev overlay, and ${pairs.length} WCAG AA text pairs (minimum ${minimumContrast.toFixed(2)}:1).`);
