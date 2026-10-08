import type { Metadata } from "next";
import { Brand } from "@/components/brand";
import { repository } from "@/lib/site";

export const metadata: Metadata = {
  title: "Privacy · vstretch",
  description: "How vstretch handles your data: everything stays on your PC. No accounts, no telemetry, no cookies.",
};

export default function Privacy() {
  return (
    <div className="mx-auto flex min-h-dvh max-w-[1040px] flex-col px-[17px] min-[380px]:px-[23px] md:px-10">
      <header className="flex min-h-[83px] items-center justify-between gap-5 md:min-h-[100px]">
        <Brand />
        <nav aria-label="Main navigation" className="flex items-center gap-[18px] text-sm font-medium text-[#595e58] md:gap-7">
          <a href="/" className="hover:text-primary">Home</a>
          <a href="/changelog" className="hover:text-primary">Changelog</a>
          <a href={repository} target="_blank" rel="noopener noreferrer" className="hover:text-primary">GitHub</a>
        </nav>
      </header>

      <main id="main" className="mx-auto w-full max-w-[68ch] pb-[47px] pt-[31px] md:pt-[47px]">
        <h1 className="mb-2 text-[clamp(1.65rem,3vw,2rem)] font-medium leading-[1.3] tracking-[-0.035em]">Privacy policy.</h1>
        <p className="mb-7 text-xs text-muted-foreground">Effective 5 October 2026</p>

        <div className="space-y-5 text-base leading-[1.7] text-muted-foreground">
          <p><strong className="font-semibold text-foreground">Short version:</strong> vstretch collects nothing. There are no accounts, no telemetry, no cookies, and no ads. Everything the app knows lives on your PC.</p>

          <section aria-labelledby="privacy-on-device">
            <h2 id="privacy-on-device" className="mb-1.5 text-[17px] font-medium tracking-[-0.02em] text-foreground">On your device</h2>
            <p>vstretch stores its settings locally at <code className="break-all text-[13px]">%APPDATA%\vstretch\config.toml</code>, alongside a short-lived recovery record used to restore your display if the app is interrupted. It changes your primary display through Windows system calls and checks locally whether Valorant or CS2 is running to drive auto-stretch. None of this leaves your computer.</p>
          </section>

          <section aria-labelledby="privacy-network">
            <h2 id="privacy-network" className="mb-1.5 text-[17px] font-medium tracking-[-0.02em] text-foreground">Network requests</h2>
            <p>The app contacts GitHub only to check for updates (<code className="break-all text-[13px]">api.github.com</code>) and, if you accept one, to download the new release. The installer downloads the same release file and verifies its SHA-256 checksum before running it. These are ordinary encrypted web requests; we receive no personal data from them beyond what is inherent to serving a file over HTTPS.</p>
          </section>

          <section aria-labelledby="privacy-website">
            <h2 id="privacy-website" className="mb-1.5 text-[17px] font-medium tracking-[-0.02em] text-foreground">This website</h2>
            <p>These pages are static files. There is no analytics, no tracking, and no cookies. If you click a GitHub link, GitHub&apos;s own privacy policy applies from there.</p>
          </section>

          <section aria-labelledby="privacy-contact">
            <h2 id="privacy-contact" className="mb-1.5 text-[17px] font-medium tracking-[-0.02em] text-foreground">Questions</h2>
            <p>Ask on <a href={`${repository}/issues`} target="_blank" rel="noopener noreferrer" className="text-[#aa4633] underline decoration-[#dca390] underline-offset-4 hover:decoration-current">GitHub</a>. If this policy ever changes, the updated version will be posted here.</p>
          </section>
        </div>
      </main>

      <footer className="mt-auto flex flex-wrap items-center justify-between gap-4 border-t py-7 text-xs text-muted-foreground">
        <Brand footer />
        <a href="/download" className="hover:text-primary">Download</a>
        <a href="/changelog" className="hover:text-primary">Changelog</a>
        <p>Not affiliated with Riot Games or Valve.</p>
      </footer>
    </div>
  );
}
