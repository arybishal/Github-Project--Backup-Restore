import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

// https://vitejs.dev/config/
export default defineConfig({
  plugins: [react()],
  // Tauri expects a fixed port in dev and no relative asset issues in prod.
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
  },
});
