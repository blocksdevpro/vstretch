import type { Metadata, Viewport } from "next";
import { DM_Sans } from "next/font/google";
import "./globals.css";

const dmSans = DM_Sans({ subsets: ["latin"], variable: "--font-dm-sans", display: "swap" });

export const metadata: Metadata = {
  title: "vstretch · Stretch for the game. Native for the rest.",
  description: "A small Windows tray app that switches to your saved stretched resolution for Valorant and CS2, then restores your desktop when you close the game. Free and open source.",
  openGraph: {
    title: "vstretch · Stretch for the game. Native for the rest.",
    description: "Automatic resolution switching for Valorant and CS2, right from your Windows tray.",
    type: "website",
  },
};

export const viewport: Viewport = { themeColor: "#fbfaf7" };

export default function RootLayout({ children }: Readonly<{ children: React.ReactNode }>) {
  return <html lang="en"><body className={dmSans.variable}>{children}</body></html>;
}
