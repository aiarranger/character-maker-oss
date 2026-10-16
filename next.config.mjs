/** @type {import('next').NextConfig} */
const nextConfig = {
  reactStrictMode: true, devIndicators: false,
  async redirects() {
    return [{ source: '/', destination: '/character-maker', permanent: false }];
  },
};
export default nextConfig;
