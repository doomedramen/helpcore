import type { Metadata } from "next";
import { AuthProvider } from "@/context/auth";
import { ThemeProvider } from "@/context/theme";
import "./globals.css";

export const metadata: Metadata = {
  title: "helpcore",
  description: "Your personal AI assistant",
};

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en" suppressHydrationWarning className="font-sans">
      <body>
        <ThemeProvider>
          <AuthProvider>{children}</AuthProvider>
        </ThemeProvider>
      </body>
    </html>
  );
}
