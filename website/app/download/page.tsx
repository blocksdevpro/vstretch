import type { Metadata } from "next";
import { Brand } from "@/components/brand";
import { repository } from "@/lib/site";
import { getLatestRelease } from "@/lib/release";

export const metadata: Metadata = {
  title: "Download · vstretch",
  description: "Download the latest vstretch release for Windows 10/11 x64.",
};

export default async function Download() {
  const release = await getLatestRelease();
  const target = release?.downloadUrl ?? `${repository}/releases/latest/download/vstretch.exe`;

  return (
    <div className="mx-auto max-w-[1040px] px-[17px] min-[380px]:px-[23px] md:px-10">
      <header className="flex min-h-[83px] items-center justify-between gap-5 md:min-h-[100px]">
        <Brand />
        <nav aria-label="Main navigation" className="flex items-center gap-[18px] text-sm font-medium text-[#595e58] md:gap-7">
          <a href="/" className="hover:text-primary">Home</a>
          <a href="/changelog" className="hover:text-primary">Changelog</a>
        </nav>
      </header>

      <main id="main" className="mx-auto max-w-[60ch] pb-[47px] pt-[31px] text-center md:pt-[47px]">
        <h1 className="mb-3 text-[clamp(1.65rem,3vw,2rem)] font-medium leading-[1.3] tracking-[-0.035em]">Starting your download.</h1>
        <p className="mb-2 text-base leading-relaxed text-muted-foreground">
          {release ? `${release.version} · ${release.size} · Windows 10/11 x64` : "Latest release · Windows 10/11 x64"}
        </p>
        <p className="mb-7 text-base leading-relaxed text-muted-foreground">If it doesn&apos;t start, use the button below.</p>
        <p>
          <a
            href={target}
            className="inline-flex min-h-[46px] items-center rounded-full bg-primary px-5 py-3 text-sm font-semibold text-white shadow-[0_3px_0_#8e472f12] transition-transform hover:-translate-y-0.5 hover:bg-[#aa4633]"
          >
            Download vstretch.exe
          </a>
        </p>
        <p className="mt-7 text-xs text-muted-foreground">
          <a href="/" className="hover:text-primary">Back home</a>
          {" · "}
          <a href="/changelog" className="hover:text-primary">Changelog</a>
        </p>
        {/* Same-domain endpoint: /download redirects to the pinned release asset below. */}
        <p data-download-target className="hidden">{target}</p>
      </main>

      <footer className="flex flex-wrap items-center justify-between gap-4 border-t py-7 text-xs text-muted-foreground">
        <Brand footer />
        <p>Not affiliated with Riot Games or Valve.</p>
      </footer>

      <script dangerouslySetInnerHTML={{ __html: `setTimeout(function(){location.replace(${JSON.stringify(target)})},600)` }} />
    </div>
  );
}
