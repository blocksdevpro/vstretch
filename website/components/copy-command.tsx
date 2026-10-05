"use client";

import { useEffect, useRef, useState } from "react";
import { Check, Copy } from "lucide-react";
import { Button } from "@/components/ui/button";
import { installCommand, installPsEndpoint } from "@/lib/site";

type CopyStatus = "idle" | "copied" | "manual";

export function CopyCommand() {
  const [status, setStatus] = useState<CopyStatus>("idle");
  const codeRef = useRef<HTMLElement>(null);

  useEffect(() => {
    if (status !== "copied") return;
    const timer = window.setTimeout(() => setStatus("idle"), 3500);
    return () => window.clearTimeout(timer);
  }, [status]);

  async function copy() {
    try {
      await navigator.clipboard.writeText(installCommand);
      setStatus("copied");
    } catch {
      if (codeRef.current) {
        const range = document.createRange();
        range.selectNodeContents(codeRef.current);
        const selection = window.getSelection();
        selection?.removeAllRanges();
        selection?.addRange(range);
      }
      setStatus("manual");
    }
  }

  return (
    <div className="pb-3 text-[#786453]">
      <p className="mb-3 text-base leading-relaxed">Install once. Open from Start. No admin needed.</p>
      <div className="mb-3 flex flex-wrap items-end gap-3 rounded-lg border border-[#e9d9cb] bg-[#fffbf7] p-4 sm:flex-nowrap">
        <code ref={codeRef} className="min-w-0 flex-1 basis-full break-all text-[13px] leading-7 text-[#765f4c] sm:basis-auto">{installCommand}</code>
        <Button variant="ghost" size="sm" className="ml-auto shrink-0 text-xs text-[#765f4c]" onClick={copy} aria-label="Copy PowerShell install command">
          {status === "copied" ? <Check /> : <Copy />}{status === "copied" ? "Copied" : "Copy"}
        </Button>
      </div>
      <a className="text-sm text-[#aa4633] underline decoration-[#dca390] underline-offset-4 hover:decoration-current" href={installPsEndpoint}>Read the install script</a>
      <p role="status" aria-live="polite" className="mt-2 min-h-5 text-xs text-[#765f4c]">{status === "copied" ? "Command copied." : status === "manual" ? "Command selected. Press Ctrl+C to copy." : ""}</p>
    </div>
  );
}
