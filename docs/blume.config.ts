import { defineConfig } from "blume";

const brand = {
  black: "oklch(0.170 0.008 250)",
  orange: "oklch(0.610 0.112 47)",
  white: "oklch(0.950 0.006 60)",
} as const;

export default defineConfig({
  ai: {
    llmsTxt: {
      enabled: true,
      openapi: false,
    },
  },
  content: {
    sources: [
      {
        root: "md",
        type: "filesystem",
      },
      {
        limit: 50,
        owner: "deepso7",
        prefix: "changelog",
        repo: "minip2p",
        type: "github-releases",
      },
    ],
  },
  deployment: {
    adapter: "cloudflare",
    site: "https://minip2p.com",
  },
  description:
    "A minimal, caller-driven libp2p implementation in Rust, built around QUIC and Sans-I/O state machines.",
  github: {
    branch: "main",
    dir: "docs",
    owner: "deepso7",
    repo: "minip2p",
  },
  lastModified: true,
  logo: {
    image: "/logo.svg",
    text: "minip2p",
  },
  markdown: {
    code: {
      icons: true,
      wrap: false,
    },
  },
  navigation: {
    tabs: [
      { label: "Docs", path: "/" },
      { label: "Changelog", path: "/changelog" },
    ],
  },
  seo: {
    agentReadability: true,
    contentSignals: {
      aiInput: true,
      aiTrain: true,
      search: true,
    },
    og: {
      enabled: true,
      logo: "/logo.svg",
      palette: {
        accent: brand.orange,
        background: brand.white,
        foreground: brand.black,
      },
    },
    robots: true,
    sitemap: true,
    structuredData: true,
    x: { creator: "@deepso7", handle: "@deepso7" },
  },
  theme: {
    accent: brand.orange,
    background: {
      dark: brand.black,
      light: brand.white,
    },
    mode: "system",
    radius: "sm",
  },
  title: "minip2p",
});
