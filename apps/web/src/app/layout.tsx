import type { Metadata } from 'next';
import { AuthProvider } from '@/context/auth';
import './globals.css';

export const metadata: Metadata = {
  title: 'helpcore',
  description: 'Your personal AI assistant',
};

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en">
      <body>
        <AuthProvider>{children}</AuthProvider>
      </body>
    </html>
  );
}
