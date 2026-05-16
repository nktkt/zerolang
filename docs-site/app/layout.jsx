import "./globals.css";
import { ThemeProvider } from "@/components/theme-provider";
import { SiteHeader } from "@/components/site-header";
import { DocsChat } from "@/components/docs-chat";
import { getStarCount } from "@/lib/github";

const SITE_URL = process.env.NEXT_PUBLIC_SITE_URL || "https://zerolang.ai";
const SITE_DESCRIPTION =
  "Zero is the programming language for agents: a systems language with explicit effects, predictable memory, and structured tooling.";

export const metadata = {
  metadataBase: new URL(SITE_URL),
  title: {
    default: "Zero — The programming language for agents.",
    template: "%s | Zero",
  },
  description: SITE_DESCRIPTION,
  openGraph: {
    type: "website",
    locale: "en_US",
    url: SITE_URL,
    siteName: "Zero",
    title: "Zero — The programming language for agents.",
    description: SITE_DESCRIPTION,
    images: [{ url: "/og", width: 1200, height: 630, alt: "Zero" }],
  },
  twitter: {
    card: "summary_large_image",
    title: "Zero — The programming language for agents.",
    description: SITE_DESCRIPTION,
    images: ["/og"],
  },
};

export const viewport = {
  width: "device-width",
  initialScale: 1,
};

export default async function RootLayout({ children }) {
  const stars = await getStarCount();

  return (
    <html lang="en" suppressHydrationWarning>
      <body className="bg-bg text-fg antialiased">
        <ThemeProvider>
          <SiteHeader stars={stars} />
          {children}
          <DocsChat />
        </ThemeProvider>
      </body>
    </html>
  );
}
