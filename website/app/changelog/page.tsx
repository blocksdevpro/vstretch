import type { Metadata } from "next";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { Brand } from "@/components/brand";
import { repository } from "@/lib/site";

export const metadata: Metadata = {
  title: "Changelog · vstretch",
  description: "What changed in each vstretch release.",
};

type ChangeGroup = { heading: string; items: string[] };
type ReleaseNotes = { version: string; date: string; groups: ChangeGroup[] };

function renderInline(text: string) {
  // Minimal inline markdown: `code` only. Full links stay in the source file.
  const parts = text.split("`");
  return parts.map((part, index) =>
    index % 2 === 1 ? <code key={index} className="break-all text-[13px] text-foreground">{part}</code> : <span key={index}>{part}</span>,
  );
}

async function getChangelog(): Promise<ReleaseNotes[]> {
  const raw = await readFile(resolve(process.cwd(), "../CHANGELOG.md"), "utf8");
  const releases: ReleaseNotes[] = [];
  let current: ReleaseNotes | null = null;
  let group: ChangeGroup | null = null;

  for (const line of raw.split("\n")) {
    const release = /^## \[(.+?)\] - (\d{4}-\d{2}-\d{2})/.exec(line.trim());
    if (release) {
      current = { version: release[1], date: release[2], groups: [] };
      releases.push(current);
      group = null;
      continue;
    }
    const heading = /^### (.+)/.exec(line.trim());
    if (heading && current) {
      group = { heading: heading[1], items: [] };
      current.groups.push(group);
      continue;
    }
    const item = /^- (.+)/.exec(line.trim());
    if (item && current && group) {
      group.items.push(item[1]);
      continue;
    }
    if (/^\[.+\]: https?:/.test(line.trim())) continue;
  }
  return releases;
}

export default async function Changelog() {
  const releases = await getChangelog();

  return (
    <div className="mx-auto flex min-h-dvh max-w-[1040px] flex-col px-[17px] min-[380px]:px-[23px] md:px-10">
      <header className="flex min-h-[83px] items-center justify-between gap-5 md:min-h-[100px]">
        <Brand />
        <nav aria-label="Main navigation" className="flex items-center gap-[18px] text-sm font-medium text-[#595e58] md:gap-7">
          <a href="/" className="hover:text-primary">Home</a>
          <a href="/download" className="hover:text-primary">Download</a>
          <a href={repository} target="_blank" rel="noopener noreferrer" className="hover:text-primary">GitHub</a>
        </nav>
      </header>

      <main id="main" className="mx-auto w-full max-w-[68ch] pb-[47px] pt-[31px] md:pt-[47px]">
        <h1 className="mb-2 text-[clamp(1.65rem,3vw,2rem)] font-medium leading-[1.3] tracking-[-0.035em]">Changelog.</h1>
        <p className="mb-7 text-base leading-relaxed text-muted-foreground">New in each release. Full history lives in <a href={`${repository}/blob/main/CHANGELOG.md`} target="_blank" rel="noopener noreferrer" className="text-[#aa4633] underline decoration-[#dca390] underline-offset-4 hover:decoration-current">CHANGELOG.md</a>.</p>

        <div className="space-y-3">
          {releases.map((release, index) => (
            <article key={release.version} className="rounded-[14px] border border-[#edede5] bg-[#f5f4ef] p-[18px] md:p-5">
              <div className="mb-3 flex flex-wrap items-center gap-2.5">
                <h2 className="text-[17px] font-medium tracking-[-0.02em]">v{release.version}</h2>
                {index === 0 && <span className="rounded-full bg-[#f9e7dd] px-2.5 py-1 text-xs font-semibold text-[#a64432]">Latest</span>}
                <span className="text-xs text-muted-foreground">{release.date}</span>
                <a href={`${repository}/releases/tag/v${release.version}`} target="_blank" rel="noopener noreferrer" className="ml-auto text-xs text-[#aa4633] underline decoration-[#dca390] underline-offset-4 hover:decoration-current">Release</a>
              </div>
              <div className="space-y-4">
                {release.groups.map((group) => (
                  <section key={group.heading} aria-label={group.heading}>
                    <h3 className="mb-1.5 text-sm font-medium text-[#596050]">{group.heading}</h3>
                    <ul className="list-disc space-y-1.5 pl-5 text-base leading-[1.7] text-muted-foreground">
                      {group.items.map((item) => <li key={item}>{renderInline(item)}</li>)}
                    </ul>
                  </section>
                ))}
              </div>
            </article>
          ))}
        </div>
      </main>

      <footer className="mt-auto flex flex-wrap items-center justify-between gap-4 border-t py-7 text-xs text-muted-foreground">
        <Brand footer />
        <a href="/download" className="hover:text-primary">Download</a>
        <a href="/privacy.html" className="hover:text-primary">Privacy</a>
        <p>Not affiliated with Riot Games or Valve.</p>
      </footer>
    </div>
  );
}
