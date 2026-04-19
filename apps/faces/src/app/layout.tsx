import type { Metadata } from "next";
import "./globals.css";

export const metadata: Metadata = {
  title: "donto · faces",
  description:
    "Three lenses on donto's bitemporal cube: Stratigraph, Rashomon Hall, Probe.",
};

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en" className="font-mono">
      <body className="bg-paper text-ink">{children}</body>
    </html>
  );
}
