import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import vueDevTools from 'vite-plugin-vue-devtools'

// https://vite.dev/config/
export default defineConfig(({ command, mode }) => {
  const plugins = [vue()]

  // Only add devtools in dev server mode, not during builds
  if (command === 'serve') {
    plugins.push(vueDevTools())
  }

  return {
    plugins,
    build: {
      outDir: '../www-admin',
      emptyOutDir: true, // also necessary
    }
  }
})
