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
  // data/: the world's saves (docs/MUNDO.md), written by the server while it runs
  server: { allowedHosts: ['.trycloudflare.com'], watch: { ignored: ['**/data/**'] } },
});
