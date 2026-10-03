import { cn } from "@/lib/utils";

export function BrandMark({ small = false }: { small?: boolean }) {
  return <span aria-hidden="true" className={cn("grid shrink-0 place-items-center bg-brand pr-0.5 font-bold italic tracking-[-0.08em] text-white", small ? "size-[22px] rounded-md text-lg" : "size-[30px] rounded-[9px] text-[25px]")}>v</span>;
}

export function Brand({ footer = false }: { footer?: boolean }) {
  return (
    <a href="#" aria-label="vstretch home" className={cn("inline-flex items-center gap-2.5 font-semibold tracking-[-0.07em] text-foreground", footer ? "text-xl" : "text-[25px]")}>
      <BrandMark small={footer} />
      <span>vstretch<span className="text-primary">.</span></span>
    </a>
  );
}
