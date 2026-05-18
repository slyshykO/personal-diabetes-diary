import {defineConfig} from 'vite'
import {viteSingleFile} from 'vite-plugin-singlefile'
import {compression} from 'vite-plugin-compression2'
import tailwindcss from '@tailwindcss/vite';

export default defineConfig({
    server: {
        proxy: {
            '/api': {
                target: 'http://127.0.0.1:8080',
                changeOrigin: true,
            },
        },
    },
    plugins: [
        tailwindcss(), 
        viteSingleFile(), 
        compression({algorithms: ['gzip']})
    ],
})
