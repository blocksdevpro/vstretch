import { cn } from "@/lib/utils";

export function BrandMark({ small = false }: { small?: boolean }) {
  return (
    <span aria-hidden="true" className={cn("grid shrink-0 place-items-center", small ? "size-[22px]" : "size-[30px]")}>
      <svg viewBox="0 0 48 48" className={cn("size-full", small ? "rounded-md" : "rounded-[9px]")}>
        <rect width="48" height="48" rx="13" fill="#df755e" />
        <path d="M11 14l9 20h7l10-20h-8l-5 12-5-12z" fill="white" />
      </svg>
    </span>
  );
}

export function Brand({ footer = false }: { footer?: boolean }) {
  return (
    <a href="#" aria-label="vstretch home" className={cn("inline-flex items-center gap-2.5 font-semibold tracking-[-0.07em] text-foreground", footer ? "text-xl" : "text-[25px]")}>
      <BrandMark small={footer} />
      <span>vstretch<span className="text-primary">.</span></span>
    </a>
  );
}
