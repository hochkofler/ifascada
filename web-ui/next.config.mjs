/** @type {import('next').NextConfig} */
const nextConfig = {
  reactStrictMode: true,
  async rewrites() {
    const upstream = process.env.CENTRAL_API_UPSTREAM || "http://127.0.0.1:8088";
    return [
      {
        source: "/api/:path*",
        destination: `${upstream}/api/:path*`,
      },
      {
        source: "/health/:path*",
        destination: `${upstream}/health/:path*`,
      },
    ];
  },
};

export default nextConfig;
