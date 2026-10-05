import Image from "next/image";
import { ChevronDown, Download, PanelsTopLeft } from "lucide-react";
import { Brand } from "@/components/brand";
import { TrayPreview } from "@/components/tray-preview";
import { CopyCommand } from "@/components/copy-command";
import { Disclosure } from "@/components/disclosure";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Accordion, AccordionContent, AccordionItem, AccordionTrigger } from "@/components/ui/accordion";
import { changelogEndpoint, downloadEndpoint, repository } from "@/lib/site";
import { getLatestRelease, type Release } from "@/lib/release";

const steps = [
  { number: "01", title: "Pick your stretch.", lines: ["Choose a tray preset.", "vstretch remembers it."] },
  { number: "02", title: "Open the game.", lines: ["Valorant or CS2 gains focus.", "Your stretch applies automatically."] },
  { number: "03", title: "Back to your desktop.", lines: ["Close the game.", "Your previous resolution returns."] },
];

const questions = [
  { id: "anti-cheat", question: "Is this safe with Vanguard / VAC?", answer: <>vstretch changes Windows display settings and checks game names and focus. It does not read or write game memory, inject code, or modify game files. We cannot guarantee anti-cheat decisions or claim Riot/Valve approval. <a href={`${repository}/tree/main/src`} target="_blank" rel="noopener noreferrer" className="text-primary underline underline-offset-4">Review the source</a>.</> },
  { id: "in-game", question: "Do I still set my in-game resolution?", answer: <>Yes. Match it to your stretch preset. vstretch changes your primary Windows display. If you see black bars, enable full-screen scaling in the game or your GPU control panel. It does not change hitboxes.</> },
  { id: "alt-tab", question: "What happens when I Alt+Tab?", answer: <>Stretch stays active until the game closes. Enable <strong className="font-semibold">Restore desktop on Alt+Tab</strong> in the tray menu to restore your desktop when you switch away and reapply stretch when you return.</> },
  { id: "startup", question: "Can it start with Windows?", answer: <>Yes, by default in v1.2.0. Turn off <strong className="font-semibold">Start with Windows</strong> in the tray to opt out. Your choice is remembered.</> },
  { id: "manual", question: "Can I switch manually?", answer: <>Choose Native or Stretch from the tray menu, bind <code>vstretch --auto</code> to a hotkey, or open <code>vstretch --tui</code>. In v1.1.1, <code>vstretch</code> opens the terminal. <a href={`${repository}#usage`} target="_blank" rel="noopener noreferrer" className="text-primary underline underline-offset-4">Usage guide</a>.</> },
];

const textLink = "text-sm text-[#aa4633] underline decoration-[#dca390] underline-offset-4 hover:decoration-current";
const downloadButton = "h-auto min-h-[46px] rounded-full px-5 py-3 text-sm font-semibold shadow-[0_3px_0_#8e472f12] transition-transform hover:-translate-y-0.5 hover:bg-[#aa4633]";

function GitHubMark() {
  return <svg viewBox="0 0 24 24" aria-hidden="true" className="size-[17px] fill-current"><path d="M12 .8a11.2 11.2 0 0 0-3.54 21.82c.56.1.77-.24.77-.54v-2.1c-3.14.68-3.8-1.34-3.8-1.34-.5-1.3-1.25-1.65-1.25-1.65-1.03-.7.08-.69.08-.69 1.14.08 1.74 1.17 1.74 1.17 1.01 1.73 2.65 1.23 3.3.94.1-.73.4-1.23.73-1.51-2.51-.29-5.15-1.26-5.15-5.58 0-1.23.44-2.23 1.16-3.02-.12-.28-.5-1.43.11-2.99 0 0 .95-.3 3.09 1.16A10.8 10.8 0 0 1 12 6.08c.96 0 1.93.13 2.83.38 2.15-1.46 3.09-1.16 3.09-1.16.62 1.56.23 2.71.12 2.99.72.79 1.16 1.79 1.16 3.02 0 4.33-2.65 5.28-5.17 5.56.41.35.77 1.04.77 2.09v3.12c0 .3.2.65.78.54A11.2 11.2 0 0 0 12 .8Z" /></svg>;
}

function DownloadDetails({ release }: { release: Release | null }) {
  return <p className="mt-3 text-xs leading-relaxed text-muted-foreground">{release ? `${release.version} · ${release.size} · ` : "Latest release · "}Windows 10/11 x64{release?.terminalOnly ? " · Terminal" : ""}</p>;
}

export default async function Home() {
  const release = await getLatestRelease();
  const executableUrl = downloadEndpoint;
  const releaseUrl = release?.releaseUrl ?? `${repository}/releases/latest`;
  return (
    <>
      <a href="#main" className="absolute -top-24 left-5 z-50 rounded-lg bg-card p-3 focus:top-3">Skip to content</a>
      <div className="mx-auto max-w-[1040px] px-[17px] min-[380px]:px-[23px] md:px-10">
        <header className="flex min-h-[83px] items-center justify-between gap-5 md:min-h-[100px]">
          <Brand />
          <nav aria-label="Main navigation" className="flex items-center gap-[18px] text-sm font-medium text-[#595e58] md:gap-7">
            <a href="#how-it-works" className="hidden hover:text-primary sm:block">How it works</a>
            <a href="#questions" className="hover:text-primary">FAQ</a>
            <a href={changelogEndpoint} className="hidden hover:text-primary sm:block">Changelog</a>
            <a href={repository} target="_blank" rel="noopener noreferrer" className="flex items-center gap-1.5 hover:text-primary"><GitHubMark />GitHub</a>
          </nav>
        </header>

        <main id="main">
          <section aria-labelledby="hero-title" className="pb-[38px] pt-[31px] text-center md:pb-[51px] md:pt-[47px]">
            <Badge variant="outline" className="gap-2 rounded-full bg-[#f6f5ef] px-3 py-1.5 text-xs font-medium text-muted-foreground"><PanelsTopLeft aria-hidden="true" className="size-3.5" />Free &amp; open source · Windows x64</Badge>
            <h1 id="hero-title" className="mb-5 mt-6 text-[clamp(2.2rem,5.3vw,3.875rem)] font-semibold leading-[1.12] tracking-[-0.045em]">Stretch for the game.<br /><span className="text-primary">Native for the rest.</span></h1>
            <p className="mx-auto max-w-[38ch] text-base leading-[1.7] text-muted-foreground md:text-[17px]">Automatic resolution switching for Valorant and CS2, right from your Windows tray.</p>
            <div className="mt-7 flex flex-col items-stretch justify-center gap-3 min-[380px]:flex-row min-[380px]:items-center">
              <Button asChild className={downloadButton}><a href={executableUrl}><Download aria-hidden="true" />Download vstretch</a></Button>
              <Button asChild variant="outline" className="h-auto min-h-[46px] rounded-full bg-card px-5 py-3 text-sm font-semibold text-[#545a52] hover:bg-secondary"><a href="#try-it">Try the preview</a></Button>
            </div>
            <DownloadDetails release={release} />
            <TrayPreview />
          </section>

          <section id="how-it-works" aria-labelledby="steps-title" className="border-t py-[34px] md:py-[46px]">
            <h2 id="steps-title" className="mb-2 text-center text-[clamp(1.65rem,3vw,2rem)] font-medium leading-[1.3] tracking-[-0.035em]">Set it once. Skip the settings.</h2>
            <p className="mb-7 text-center text-xs text-muted-foreground">Auto-stretch in v1.2.0</p>
            <div className="grid gap-3 md:grid-cols-3 md:gap-3.5">
              {steps.map((step) => <article key={step.number} className="grid grid-cols-[30px_1fr] gap-x-3 rounded-[14px] border border-[#edede5] bg-[#f5f4ef] p-[18px] md:block md:p-5">
                <span className="row-span-2 mt-0.5 grid size-7 place-items-center rounded-full bg-[#f9e7dd] text-xs font-semibold text-[#a64432]">{step.number}</span>
                <h3 className="mb-1.5 text-[17px] font-medium tracking-[-0.02em] md:mb-2 md:mt-4">{step.title}</h3>
                <p className="text-base leading-[1.65] text-muted-foreground">{step.lines[0]}<br />{step.lines[1]}</p>
              </article>)}
            </div>
            <Disclosure title="Prefer hotkeys or the terminal?" className="mx-auto mt-5 max-w-[750px] border-b text-[#596050]">
              <div className="grid items-center gap-5 pb-5 sm:grid-cols-[0.7fr_1fr] sm:gap-6">
                <div><p className="mb-3 text-base leading-relaxed text-muted-foreground">Bind <code className="break-all text-[13px]">vstretch --auto</code> to your own hotkey. Open <code className="break-all text-[13px]">vstretch --tui</code> for native settings and updates.</p><a href={`${repository}#usage`} target="_blank" rel="noopener noreferrer" className={textLink}>Usage guide</a></div>
                <figure className="min-w-0 rounded-[10px] border border-[#e1e2d9] bg-[#eaeae4] p-[7px]"><Image src="/assets/vstretch-terminal.png" width={1290} height={746} alt="vstretch terminal interface with current display information and stretch and native actions." className="h-auto w-full rounded-sm" /></figure>
              </div>
            </Disclosure>
          </section>

          <section id="download" aria-labelledby="install-title" className="mb-[37px] mt-3 rounded-[17px] border border-[#f1e1d7] bg-[#f7ece4] px-[22px] pb-3 pt-[22px] md:mb-[46px] md:px-8 md:pt-7">
            <div className="flex flex-col items-start justify-between gap-[17px] pb-[22px] lg:flex-row lg:items-center lg:gap-7">
              <div><h2 id="install-title" className="mb-2 text-[25px] font-medium leading-[1.3] tracking-[-0.035em] md:text-[27px]">Ready for your next match.</h2><p className="text-base leading-relaxed text-[#786453]">{release?.terminalOnly ? "Terminal release now. Tray app coming in v1.2.0." : "Download. Double-click. Find it beside your clock."}</p></div>
              <Button asChild className={`${downloadButton} shrink-0`}><a href={executableUrl}><Download aria-hidden="true" />Get vstretch.exe</a></Button>
            </div>
            <div className="mb-2 flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-[#765f4c]"><span>Free &amp; open source</span><a href={release?.sourceUrl ?? repository} className={textLink}>Source</a><a href={releaseUrl} className={textLink}>SHA-256</a><a href={changelogEndpoint} className={textLink}>Changelog</a></div>
            <details id="download-checks" className="group border-t border-[#e3cebf] text-[#765f4c]">
              <summary className="flex min-h-[40px] cursor-pointer list-none items-center justify-between gap-5 py-2 text-xs font-normal [&::-webkit-details-marker]:hidden">Verify download<ChevronDown aria-hidden="true" className="size-3 shrink-0 transition-transform group-open:rotate-180" /></summary>
              <div className="max-w-[60ch] space-y-3 pb-4 text-sm leading-relaxed">
                <p>Windows may ask you to confirm a new app. To check this file, compare its hash with the <a href={releaseUrl} className={textLink}>release checksum</a>.</p>
                <pre className="overflow-x-auto rounded-lg border border-[#e3cebf] bg-[#fffbf7] p-3 text-sm text-foreground"><code>Get-FileHash .\vstretch.exe -Algorithm SHA256</code></pre>
                {release?.sha256 && <div><p className="mb-1.5 text-xs">SHA-256 · {release.version}</p><code className="block break-all text-[13px] text-foreground">{release.sha256}</code></div>}
                <p className="text-xs">Signed builds are on the way. Keep Windows protection enabled.</p>
              </div>
            </details>
            <Disclosure title="Install with PowerShell" className="border-t border-[#e3cebf] text-[#765f4c]"><CopyCommand /></Disclosure>
          </section>

          <section id="questions" aria-labelledby="faq-title" className="grid gap-[22px] pb-[33px] md:grid-cols-[0.8fr_1.2fr] md:gap-[45px] md:pb-[47px]">
            <div><h2 id="faq-title" className="mb-3 text-[26px] font-medium leading-[1.3] tracking-[-0.035em]">A few quick answers.</h2><a href={`${repository}/issues`} target="_blank" rel="noopener noreferrer" className={textLink}>Ask on GitHub</a></div>
            <Accordion type="multiple" className="border-t">
              {questions.map((item) => <AccordionItem key={item.id} value={item.id}><AccordionTrigger className="gap-5 py-[17px] text-left text-sm font-medium text-[#596050] hover:no-underline">{item.question}</AccordionTrigger><AccordionContent className="pr-4 text-base leading-[1.7] text-muted-foreground">{item.answer}</AccordionContent></AccordionItem>)}
            </Accordion>
          </section>
        </main>

        <footer className="flex flex-wrap items-center justify-between gap-4 border-t py-7 text-xs text-muted-foreground">
          <Brand footer />
          <p>By <a href="https://www.blocksdev.pro/" target="_blank" rel="noopener noreferrer" className="hover:text-primary">blocksdev</a></p>
          {release && <a href={executableUrl} title={`${release.version} downloads, checked ${release.checkedAt}`} className="hover:text-primary">{release.downloads.toLocaleString("en-US")} downloads</a>}
          <a href={`${repository}/blob/main/LICENSE`} target="_blank" rel="noopener noreferrer" className="hover:text-primary">MIT license</a>
          <a href={changelogEndpoint} className="hover:text-primary">Changelog</a>
          <a href="/privacy.html" className="hover:text-primary">Privacy</a>
          <p className="basis-full">Not affiliated with Riot Games or Valve.</p>
        </footer>
      </div>
    </>
  );
}
