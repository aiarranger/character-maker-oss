// 公開用の組み立て（gyogun.yukikau.jp。tools/gyogun-site.mjs が GYOGUN_SITE=1 で別の場所に組んだ写しから呼ぶ）は、静的書き出しにする。
// ふだんの pnpm dev・pnpm build（と埋め込み用のパッケージ）は今までどおり。docs/gyogun-spec.md の「公開先」
const site = process.env.GYOGUN_SITE === '1';

/** @type {import('next').NextConfig} */
const nextConfig = site ? {
  reactStrictMode: true, devIndicators: false,
  output: 'export',
} : {
  reactStrictMode: true, devIndicators: false,
  async redirects() {
    return [{ source: '/', destination: '/character-maker', permanent: false }];
  },
};
export default nextConfig;
