import type { NextConfig } from "next";

const config: NextConfig = {
  reactStrictMode: true,
  // Allow @donto/client (workspace package) to be transpiled by Next.
  transpilePackages: ["@donto/client"],
  // Surface the dontosrv URL into the client bundle.
  env: {
    NEXT_PUBLIC_DONTOSRV_URL:
      process.env.DONTOSRV_URL ?? "http://localhost:7878",
  },
};

export default config;
