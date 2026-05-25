import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import tailwindcss from '@tailwindcss/vite';
import path from 'path';
import { existsSync } from 'node:fs';

const rootShared = path.resolve(__dirname, '../shared');

function sharedResolverPlugin() {
    return {
        name: 'shared-resolver',
        resolveId(source: string, importer: string | undefined) {
            if (!importer || !source.startsWith('..')) return null;
            const resolved = path.resolve(path.dirname(importer), source);
            // If resolved path is already inside root shared/, let it through
            if (resolved.startsWith(rootShared + path.sep)) {
                return resolved;
            }
            // Natural path doesn't exist → try root shared/ as fallback
            if (!existsSync(resolved)) {
                const basename = path.basename(source);
                const candidate = path.resolve(rootShared, basename);
                if (existsSync(candidate)) {
                    return candidate;
                }
            }
            return null;
        },
    };
}

export default defineConfig({
    plugins: [sharedResolverPlugin(), react(), tailwindcss()],
    base: './',
    build: { outDir: 'dist', minify: false, sourcemap: true },
    resolve: {
        alias: {
            '@': path.resolve(__dirname, 'src'),
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