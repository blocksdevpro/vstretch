import { repository } from "@/lib/site";

type Asset = { name: string; size: number; digest: string | null; download_count: number };
type GitHubRelease = { tag_name: string; draft: boolean; prerelease: boolean; assets: Asset[] };

export type Release = {
  version: string;
  size: string;
  downloadUrl: string;
  releaseUrl: string;
  sourceUrl: string;
  sha256: string | null;
  totalDownloads: number;
  checkedAt: string;
  terminalOnly: boolean;
};

// Static exports resolve this at build time. Pin the download and checksum to
// the same release so a later GitHub release cannot invalidate the shown hash.
export async function getLatestRelease(): Promise<Release | null> {
  try {
    const releases: GitHubRelease[] = [];
    for (let page = 1; ; page += 1) {
      const response = await fetch(`https://api.github.com/repos/blocksdevpro/vstretch/releases?per_page=100&page=${page}`, {
        headers: { Accept: "application/vnd.github+json", "X-GitHub-Api-Version": "2022-11-28" },
        signal: AbortSignal.timeout(10000),
      });
      if (!response.ok) throw new Error("GitHub releases request failed");
      const pageItems = await response.json();
      if (!Array.isArray(pageItems)) throw new Error("GitHub releases response is incomplete");
      releases.push(...pageItems as GitHubRelease[]);
      if (pageItems.length < 100) break;
    }
    // The list is newest-first; match the stable release used by /releases/latest.
    const latest = releases.find((release) => !release.draft && !release.prerelease);
    if (!latest) throw new Error("GitHub has no stable release");
    const version = /^v?(\d+)\.(\d+)\.(\d+)$/.exec(latest.tag_name);
    const asset = latest.assets.find((item) => item.name === "vstretch.exe");
    const counts = releases.flatMap((release) => release.assets.map((item) => item.download_count));
    const totalDownloads = counts.reduce((total, count) => total + count, 0);
    if (!version || !asset || !Number.isSafeInteger(asset.size) || asset.size <= 0
      || !counts.every((count) => Number.isSafeInteger(count) && count >= 0)
      || !Number.isSafeInteger(totalDownloads)) {
      throw new Error("GitHub release metadata is incomplete");
    }
    const tag = encodeURIComponent(latest.tag_name);
    return {
      version: `v${version[1]}.${version[2]}.${version[3]}`,
      size: `${(asset.size / 1000000).toFixed(2)} MB`,
      downloadUrl: `${repository}/releases/download/${tag}/vstretch.exe`,
      releaseUrl: `${repository}/releases/tag/${tag}`,
      sourceUrl: `${repository}/tree/${tag}`,
      sha256: /^sha256:[a-f0-9]{64}$/.test(asset.digest ?? "") ? asset.digest!.slice(7) : null,
      totalDownloads,
      checkedAt: new Date().toISOString().slice(0, 10),
      terminalOnly: Number(version[1]) < 1 || (Number(version[1]) === 1 && Number(version[2]) < 2),
    };
  } catch {
    console.warn("Release details unavailable; linking to GitHub for current download information.");
    return null;
  }
}
