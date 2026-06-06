import type { NextConfig } from "next";

const config: NextConfig = {
  trailingSlash: true,
  images: { unoptimized: true },
};

// Static export for the Docker build (Rust server serves the out/ directory).
// In dev, the Next.js dev server runs normally and proxies /api/* to the Rust server.
if (process.env.NEXT_EXPORT === "true") {
  config.output = "export";
} else {
  config.rewrites = async () => [
    {
      source: "/api/:path*",
      destination: `${process.env.HELPCORE_URL ?? "http://localhost:3000"}/api/:path*`,
    },
  ];
}

export default config;
