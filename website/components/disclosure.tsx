"use client";

import type { ReactNode } from "react";
import { Plus } from "lucide-react";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { cn } from "@/lib/utils";

export function Disclosure({ title, children, className }: { title: string; children: ReactNode; className?: string }) {
  return (
    <Collapsible className={className}>
      <CollapsibleTrigger className={cn("group flex min-h-[52px] w-full items-center justify-between gap-5 py-3 text-left text-sm font-medium")}>
        {title}<Plus aria-hidden="true" className="size-3.5 shrink-0 transition-transform group-data-[state=open]:rotate-45" />
      </CollapsibleTrigger>
      <CollapsibleContent>{children}</CollapsibleContent>
    </Collapsible>
  );
}
