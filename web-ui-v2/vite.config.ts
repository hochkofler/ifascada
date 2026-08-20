import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { tanstackRouter } from "@tanstack/router-plugin/vite";
import path from "node:path";

const __dirname = import.meta.dirname;

export default defineConfig({
  plugins: [tanstackRouter({ target: "react", autoCodeSplitting: true }), react(), tailwindcss()],
  resolve: {
    alias: { "@": path.resolve(__dirname, "./src") },
  },
  server: {
    port: 3002,
    proxy: {
      "/api": { target: "http://127.0.0.1:8088", changeOrigin: true },
      "/health": { target: "http://127.0.0.1:8088", changeOrigin: true },
    },
  },
});
