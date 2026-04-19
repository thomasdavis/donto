import type { Config } from "tailwindcss";

export default {
  content: ["./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      fontFamily: {
        mono: [
          "ui-monospace", "SF Mono", "Menlo", "Consolas",
          "Liberation Mono", "monospace",
        ],
      },
      colors: {
        // Hand-picked palette: paper-grey background, ochre accent. Reads
        // like a notary's ledger more than a SaaS dashboard.
        paper:    "#0e0d0c",
        ink:      "#d8d2c8",
        muted:    "#6a655e",
        rule:     "#2a2723",
        panel:    "#131210",
        accent:   "#f0c674",
        retract:  "#b0584c",
        derived:  "#82a45a",
      },
    },
  },
  plugins: [],
} satisfies Config;
