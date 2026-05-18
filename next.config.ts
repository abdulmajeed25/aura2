import type { NextConfig } from "next";

const internalHost = process.env.TAURI_DEV_HOST || "localhost";

const nextConfig: NextConfig = {
  output: "export",
  images: { unoptimized: true },
  assetPrefix: process.env.NODE_ENV === "production" ? null : `http://${internalHost}:3000`,
  reactStrictMode: true,
};

export default nextConfig;
