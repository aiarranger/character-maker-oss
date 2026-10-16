import type { Metadata } from "next";
import "./globals.css";
export const metadata: Metadata = { title: "Character Studio · キャラクターメイク", description: "キャラクターを選んで、その場でカスタマイズする" };
export default function RootLayout({ children }: { children: React.ReactNode }) {
  return <html lang="ja"><body>{children}</body></html>;
}
