# vstretch landing page

A Next.js App Router site for Windows FPS players, with TypeScript, Tailwind CSS 4, and shadcn/ui components generated from the official registry. The landing page uses short copy, a consistent type scale, and an interactive preview. A subtle verify-download helper with source and checksum links stays collapsed. Setup instructions, terminal usage, and FAQ answers open on demand.

The page has a centered hero, crosshair preview, numbered step cards, and a two-column FAQ. The tray preview uses React state and shadcn Toggle Group and Select controls. Its grid and crosshair stretch horizontally when selected, without changing a visitor's display. In-game scaling varies. Tailwind handles the responsive layout and warm color theme. DM Sans is bundled by `next/font`. Body text is 16px, with secondary information in collapsed disclosures.

The copy follows the current Rust app and the root `README.md`. At build time, `lib/release.ts` reads GitHub's latest stable release for its version, executable size, SHA-256, and actual asset download count. Source and checksum links are pinned to that release so the hash matches. If GitHub is unavailable, the page links to the latest release without inventing metadata or counts. Rebuild the production deployment after publishing a GitHub release to refresh the static metadata. The page distinguishes pre-v1.2.0 terminal releases from tray releases.

Retrieval uses same-domain endpoints: `/download` redirects to the pinned `vstretch.exe` asset, while `/install.ps1` and `/install.sh` serve the repository installers (`scripts/sync-site-assets.mjs` copies them to `public/` on `prebuild`). The install command is `irm https://vstretch.blocksdev.pro/install.ps1 | iex` and Git Bash uses `curl -fsSL https://vstretch.blocksdev.pro/install.sh | sh`. `/changelog` renders the root `CHANGELOG.md` at build time.

The current published executable is unsigned and SignPath signing is in progress. The verify helper stays intentionally subtle until signed builds ship; flip it to a signed/publisher message once releases are signed. `devIndicators: false` hides Next.js's development indicator; static production exports do not ship that overlay.

## Develop

Run these commands in the `website` directory:

```powershell
npm ci
npm run dev
```

Open `http://127.0.0.1:4173`.

## Build and verify

```powershell
npm run build
npm run typecheck
npm run check
```

Next.js exports the production site to `out`, including React controls and bundled assets. Vercel's GitHub integration deploys `main` to `vstretch.blocksdev.pro` and creates preview deployments for `develop`. The `.openai/hosting.json` manifest belongs to the older Sites copy. To preview the production build locally:

```powershell
python -m http.server 4173 --bind 127.0.0.1 --directory out
```

`scripts/check-site.mjs` verifies the production export, framework dependencies, heading structure, section links, local assets, installer source, download/source consistency, safety copy, and WCAG AA text contrast for the theme's foreground/background pairs. It also checks that FAQ/setup details start collapsed and the initial page stays within a 300-word budget. Browser verification is still needed for interactive controls and responsive layout.

## Structure

- `app/page.tsx`: landing page copy and layout.
- `app/download/page.tsx`: same-domain redirect to the pinned release asset.
- `app/changelog/page.tsx`: release notes rendered from the root `CHANGELOG.md`.
- `app/globals.css`: Tailwind theme and display illustration.
- `components/tray-preview.tsx`: interactive tray preview.
- `components/copy-command.tsx`: copyable PowerShell installer.
- `components/ui`: shadcn component source.
- `lib/site.ts`: shared site endpoints, repository, and installer commands.
- `lib/release.ts`: build-time GitHub release metadata and pinned asset links.
- `scripts/sync-site-assets.mjs`: copies root installers to `public/` for `/install.ps1` and `/install.sh`.

## Design references

- [USWDS typography](https://designsystem.digital.gov/components/typography/): comfortable body text, line length, and hierarchy.
- [GOV.UK type scale](https://design-system.service.gov.uk/styles/type-scale/): consistent sizes and vertical rhythm across screen widths.
- [NN/g progressive disclosure](https://www.nngroup.com/articles/progressive-disclosure/): reveal secondary instructions on demand.
- [NN/g web reading](https://www.nngroup.com/articles/how-users-read-on-the-web/): concise, scannable copy.
- [NN/g on aesthetic and minimalist design](https://www.nngroup.com/articles/aesthetic-minimalist-design/): keep core information visible and reveal secondary information when requested.
- [W3C contrast guidance](https://www.w3.org/WAI/WCAG22/Understanding/contrast-minimum.html): readable text contrast.
- [W3C text spacing guidance](https://www.w3.org/WAI/WCAG22/Understanding/text-spacing.html): relative font sizes and flexible content containers.
