import {defineConfig} from 'vite'
import {viteSingleFile} from 'vite-plugin-singlefile'
import {compression} from 'vite-plugin-compression2'
import tailwindcss from '@tailwindcss/vite';

export default defineConfig({
    plugins: [
        tailwindcss(), 
        viteSingleFile(), 
        compression({algorithms: ['gzip']})
    ],
})