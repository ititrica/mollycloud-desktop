import 'core-js/actual/array/at'
import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { connectMolly, mollyEmbedded } from './lib/mollyBridge'
import 'streamdown/styles.css'
import 'katex/dist/katex.min.css'
import './index.css'
import { installMobileViewportGuards } from './lib/viewport'

installMobileViewportGuards()

if (!mollyEmbedded && 'serviceWorker' in navigator) {
  if (import.meta.env.PROD) {
    window.addEventListener('load', () => {
      navigator.serviceWorker.register(`${import.meta.env.BASE_URL}sw.js`).catch((error) => {
        console.error('Service worker registration failed:', error)
      })
    })
  } else {
    navigator.serviceWorker.getRegistrations().then((registrations) => {
      registrations.forEach((registration) => registration.unregister())
    })
  }
}

async function start() {
  if (mollyEmbedded) await connectMolly()
  const { default: App } = await import('./App')
  createRoot(document.getElementById('root')!).render(<StrictMode><App /></StrictMode>)
}
void start().catch(() => {
  document.getElementById('root')!.textContent = '生图工作台加载失败，请在控制台重新加载。'
})
