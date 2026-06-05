import type { Metadata } from 'next';
import { AuthProvider } from '@/context/auth';
import { ThemeProvider } from '@/context/theme';
import './globals.css';

export const metadata: Metadata = {
  title: 'helpcore',
  description: 'Your personal AI assistant',
};

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en" suppressHydrationWarning>
      <head>
        <script
          dangerouslySetInnerHTML={{
            __html: `
              try {
                const savedTheme = localStorage.getItem('helpcore-theme');
                const dark = savedTheme
                  ? savedTheme === 'dark'
                  : window.matchMedia('(prefers-color-scheme: dark)').matches;
                document.documentElement.classList.toggle('dark', dark);
                document.documentElement.style.colorScheme = dark ? 'dark' : 'light';
              } catch {}
            `,
          }}
        />
      </head>
      <body>
        <ThemeProvider>
          <AuthProvider>{children}</AuthProvider>
        </ThemeProvider>
      </body>
    </html>
  );
}
