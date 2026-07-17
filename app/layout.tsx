import type { Metadata } from "next";
import { headers } from "next/headers";
import "./globals.css";

export async function generateMetadata(): Promise<Metadata> {
  const requestHeaders = await headers();
  const host = requestHeaders.get("x-forwarded-host") ?? requestHeaders.get("host") ?? "localhost";
  const protocol = requestHeaders.get("x-forwarded-proto") ?? (host.startsWith("localhost") ? "http" : "https");
  const image = new URL("/og.png", `${protocol}://${host}`).toString();
  return {
    title: "Thinkloom — ideas into writing",
    description: "A local-first writing studio for exploring ideas, shaping drafts, and preserving your creative process.",
    icons: { icon: "/icon.png", shortcut: "/icon.png", apple: "/icon.png" },
    openGraph: { title: "Thinkloom — ideas into writing", description: "Explore, shape, and publish thoughtful writing without losing the thread of how it came together.", type: "website", images: [{ url: image, width: 1732, height: 909, alt: "Thinkloom — ideas into writing, without losing the thread." }] },
    twitter: { card: "summary_large_image", images: [image] },
  };
}

export default function RootLayout({ children }: Readonly<{ children: React.ReactNode }>) { return <html lang="en"><body>{children}</body></html>; }

