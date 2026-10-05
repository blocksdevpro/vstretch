import { repository } from "@/lib/site";

export type Release = {
  version: string;
  size: string;
  downloadUrl: string;
  releaseUrl: string;
  sourceUrl: string;
  sha256: string | null;
  downloads: number;
  checkedAt: string;
  terminalOnly: boolean;
};

// Static exports resolve this at build time. Pin the download and checksum to
// the same release so a later GitHub release cannot invalidate the shown hash.
export async function getLatestRelease(): Promise<Release | null> {
  try {
    const response = await fetch("https://api.github.com/repos/blocksdevpro/vstretch/releases/latest", {
      headers: { Accept: "application/vnd.github+json", "X-GitHub-Api-Version": "2022-11-28" },
      signal: AbortSignal.timeout(10000),
    });
    if (!response.ok) throw new Error("GitHub release request failed");
    const data = await response.json() as {
      tag_name: string;
      assets: { name: string; size: number; digest: string | null; download_count: number }[];
    };
    const version = /^v?(\d+)\.(\d+)\.(\d+)$/.exec(data.tag_name);
    const asset = data.assets.find((item) => item.name === "vstretch.exe");
    if (!version || !asset || !Number.isSafeInteger(asset.size) || asset.size <= 0
      || !Number.isSafeInteger(asset.download_count) || asset.download_count < 0) {
      throw new Error("GitHub release metadata is incomplete");
    }
    const tag = encodeURIComponent(data.tag_name);
    return {
      version: `v${version[1]}.${version[2]}.${version[3]}`,
      size: `${(asset.size / 1000000).toFixed(2)} MB`,
      downloadUrl: `${repository}/releases/download/${tag}/vstretch.exe`,
      releaseUrl: `${repository}/releases/tag/${tag}`,
      sourceUrl: `${repository}/tree/${tag}`,
      sha256: /^sha256:[a-f0-9]{64}$/.test(asset.digest ?? "") ? asset.digest!.slice(7) : null,
      downloads: asset.download_count,
      checkedAt: new Date().toISOString().slice(0, 10),
      terminalOnly: Number(version[1]) < 1 || (Number(version[1]) === 1 && Number(version[2]) < 2),
    };
  } catch {
    console.warn("Release details unavailable; linking to GitHub for current download information.");
    return null;
  }
}
