import './globals.css';
import type { Metadata, Viewport } from 'next';

const VERSION = process.env.NEXT_PUBLIC_JJ_VERSION ?? 'dev';

export const metadata: Metadata = {
  title: 'JJ',
  description: 'jj-agentic-aspect dashboard',
};

export const viewport: Viewport = {
  width: 'device-width',
  initialScale: 1,
  viewportFit: 'cover',
  themeColor: '#09090b',
};

export default function RootLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return (
    <html lang="en">
      <body>
        {children}
        <div className="fixed bottom-2 right-3 text-xs font-mono text-zinc-400 select-none pointer-events-none">
          v{VERSION}
        </div>
      </body>
    </html>
  );
}
