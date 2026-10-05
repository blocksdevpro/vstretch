import { copyFile, mkdir } from 'node:fs/promises';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const websiteRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const repoRoot = resolve(websiteRoot, '..');
const publicDir = resolve(websiteRoot, 'public');

await mkdir(publicDir, { recursive: true });
await copyFile(resolve(repoRoot, 'install.ps1'), resolve(publicDir, 'install.ps1'));
await copyFile(resolve(repoRoot, 'install.sh'), resolve(publicDir, 'install.sh'));
console.log('Synced install.ps1 and install.sh to public/');
