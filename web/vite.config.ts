import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import tailwindcss from '@tailwindcss/vite';
import path from 'path';

export default defineConfig({
    plugins: [react(), tailwindcss()],
    base: './',
    build: { outDir: 'dist', minify: false, sourcemap: true },
    resolve: {
        alias: {
            '@': path.resolve(__dirname, 'src'),
            '../../../../shared/': path.resolve(__dirname, '../shared/'),
        },
    },
    server: {
        port: 5173,
        strictPort: true,
        host: 'localhost',
        fs: {
            allow: [path.resolve(__dirname, '..')]
        },
        headers: {
            'Access-Control-Allow-Origin': '*',
        },
    },
});
