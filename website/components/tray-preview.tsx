"use client";

import { useState } from "react";
import { Check } from "lucide-react";
import { BrandMark } from "@/components/brand";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { cn } from "@/lib/utils";

const presets = {
  "1440x1080": { width: 1440, height: 1080 },
  "1280x960": { width: 1280, height: 960 },
  "1024x768": { width: 1024, height: 768 },
};
type Preset = keyof typeof presets;
type Mode = "native" | "stretch";

function isPreset(value: string): value is Preset {
  return Object.hasOwn(presets, value);
}

export function TrayPreview() {
  const [mode, setMode] = useState<Mode>("stretch");
  const [preset, setPreset] = useState<Preset>("1440x1080");
  const resolution = mode === "stretch" ? presets[preset] : { width: 1920, height: 1080 };

  return (
    <div id="try-it" className="mx-auto mt-8 max-w-[750px] rounded-[18px] border bg-card px-3 pb-3 text-left shadow-[0_10px_30px_-16px_#2b31232e] sm:mt-10 sm:px-[18px] sm:pb-[18px]">
      <div className="flex flex-wrap items-center justify-between gap-x-3 gap-y-1 px-0.5 py-4 text-xs text-muted-foreground">
        <span className="font-medium tracking-[0.06em]">ONE LITTLE TRAY ICON</span>
        <span>Interactive preview · v1.2.0 upcoming</span>
      </div>
      <div className="grid gap-3 sm:grid-cols-[1fr_1.08fr] sm:gap-4">
        <div className="rounded-[11px] border bg-[#fcfcfa] px-2.5 py-3.5">
          <div className="mx-1.5 mb-3 flex items-center gap-2 text-sm font-semibold"><BrandMark small />vstretch</div>
          <ToggleGroup type="single" orientation="vertical" spacing={1} value={mode} onValueChange={(value) => { if (value === "native" || value === "stretch") setMode(value); }} aria-label="Tray mode controls" className="flex w-full flex-col gap-1">
            {(["native", "stretch"] satisfies Mode[]).map((value) => (
              <ToggleGroupItem key={value} value={value} aria-label={value === "native" ? "Native" : "Stretch"} className="h-10 w-full justify-start gap-2 rounded-md px-2.5 text-sm font-normal text-[#596050] data-[state=on]:bg-peach data-[state=on]:text-[#a64432]">
                <Check aria-hidden="true" className={cn("size-3.5", mode !== value && "invisible")} />
                {value === "native" ? "Native" : "Stretch"}
                <span className="ml-auto text-xs">{value === "native" ? "16:9" : "4:3"}</span>
              </ToggleGroupItem>
            ))}
          </ToggleGroup>
          <div className="mt-2 flex items-center gap-3 border-t px-2.5 py-3.5 text-sm text-[#596050]">
            <label htmlFor="stretch-preset">Preset</label>
            <Select value={preset} onValueChange={(value) => { if (isPreset(value)) setPreset(value); }}>
              <SelectTrigger id="stretch-preset" aria-label="Stretch preset" className="min-w-0 flex-1 bg-white"><SelectValue /></SelectTrigger>
              <SelectContent>
                {Object.entries(presets).map(([value, resolution]) => <SelectItem key={value} value={value}>{resolution.width} × {resolution.height}</SelectItem>)}
              </SelectContent>
            </Select>
          </div>
          <div className="flex items-center gap-2 px-2.5 text-xs text-[#496751]"><Check aria-hidden="true" className="size-3.5 text-success" />Auto-stretch Valorant &amp; CS2</div>
        </div>
        <div aria-hidden="true" data-display={mode} className={cn("relative min-h-[200px] overflow-hidden rounded-[11px] border sm:min-h-[252px]", mode === "stretch" ? "border-[#f0e3d9] bg-[#fcf4ee]" : "border-[#e7eae2] bg-[#f7f9f5]")}>
          <div className="display-grid" />
          <div className="display-crosshair"><span className="absolute inset-3 rounded-full border border-success/60 bg-white/40" /><span className="absolute inset-[35px] rounded-full bg-success" /></div>
          <div className="absolute inset-x-0 bottom-6 text-center">
            <span className="text-[23px] font-medium tracking-[-0.035em] tabular-nums">{resolution.width} × {resolution.height}</span>
            <span className="mt-1 block text-xs text-muted-foreground">{mode === "stretch" ? "4:3, stretched to fill" : "Your native desktop"}</span>
          </div>
        </div>
      </div>
      <p role="status" aria-live="polite" className="sr-only">Preview: {mode === "stretch" ? "stretched" : "native"} resolution, {resolution.width} by {resolution.height}. Your actual display is unchanged.</p>
    </div>
  );
}
