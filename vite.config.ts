import { defineConfig } from 'vite';

export default defineConfig({
  root: '.',
  publicDir: 'public',
  build: {
    outDir: 'dist/client',
    target: 'es2022',
    chunkSizeWarningLimit: 4000,
  },
  worker: { format: 'es' },
  // `npm run share` (Cloudflare quick tunnel): a new random subdomain every time
  server: { allowedHosts: ['.trycloudflare.com'] },
});
