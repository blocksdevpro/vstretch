import type { NextConfig } from "next";

const nextConfig = {
  output: "export",
  devIndicators: false,
  images: { unoptimized: true },
} satisfies NextConfig;

export default nextConfig;
