// Builds the public demo: the same interface, with demo/backend.js in place of
// the host and the ship. Output is plain static files.
import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import path from 'node:path'

export default defineConfig({
  root: import.meta.dirname,
  plugins: [react()],
  resolve: { alias: { 'node:child_process': path.join(import.meta.dirname, 'no-node.js') } },
  build: { target: 'es2022', outDir: 'dist', emptyOutDir: true, sourcemap: false },
})
