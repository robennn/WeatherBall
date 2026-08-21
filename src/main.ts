import { createApp } from 'vue'
import App from './App.vue'
import './styles/ball.css'
import { initDesktopApi } from './platform'

async function bootstrap() {
  await initDesktopApi()
  createApp(App).mount('#app')
}

void bootstrap()
