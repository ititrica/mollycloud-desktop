import type { StateStorage } from 'zustand/middleware'

export const mollyEmbedded = import.meta.env.VITE_MOLLY_EMBEDDED === 'true'
let port: MessagePort
let sequence = 0
let snapshot: string | null = null
let storageErrorReported = false
const pending = new Map<number, { resolve: (value: any) => void, reject: (error: Error) => void }>()
const streams = new Map<number, ReadableStreamDefaultController<Uint8Array>>()
const streamCleanup = new Map<number, () => void>()

export function mollyRequest<T>(type: string, data: object = {}): Promise<T> {
  return new Promise((resolve, reject) => {
    const id = ++sequence
    pending.set(id, { resolve, reject })
    port.postMessage({ type, id, ...data })
  })
}

export const mollyStorage: StateStorage = {
  getItem: () => snapshot,
  setItem: (_name, value) => {
    snapshot = value
    return mollyRequest<void>('settings', { value }).then(() => { storageErrorReported = false }).catch(() => {
      if (!storageErrorReported) {
        storageErrorReported = true
        window.dispatchEvent(new CustomEvent('molly-storage-error'))
      }
    })
  },
  removeItem: () => mollyRequest<void>('settings', { value: null }),
}

export async function saveMollySettings() {
  await mollyRequest<void>('settings', { value: snapshot })
}

export type DbOperation = { store: string, method: 'get' | 'getAll' | 'getAllKeys' | 'put' | 'delete' | 'clear', value?: unknown }
export function mollyDb<T>(operations: DbOperation[]): Promise<T> {
  return mollyRequest<T>('database', { operations })
}

export async function connectMolly() {
  // 不把密钥或账户标识放在子页面 URL；只接受父窗口转交的一次性通道。
  await new Promise<void>((resolve, reject) => {
    const timeout = setTimeout(() => { window.removeEventListener('message', receive); reject(new Error('控制台连接超时，请重新打开工作台。')) }, 20000)
    const receive = (event: MessageEvent) => {
      if (event.source !== window.parent || event.data?.type !== 'molly-image-connect' || !event.ports[0]) return
      window.removeEventListener('message', receive)
      clearTimeout(timeout)
      port = event.ports[0]
      snapshot = event.data.settings ?? null
      port.onmessage = ({ data }) => {
        if (data.type === 'chunk') { streams.get(data.id)?.enqueue(new Uint8Array(data.bytes)); return }
        if (data.type === 'end') { streams.get(data.id)?.close(); streams.delete(data.id); streamCleanup.get(data.id)?.(); streamCleanup.delete(data.id); return }
        if (data.error) {
          const error = new Error(data.error)
          streams.get(data.id)?.error(error)
          streams.delete(data.id)
          streamCleanup.get(data.id)?.()
          streamCleanup.delete(data.id)
          pending.get(data.id)?.reject(error)
        } else pending.get(data.id)?.resolve(data.value)
        pending.delete(data.id)
      }
      resolve()
    }
    window.addEventListener('message', receive)
    window.parent.postMessage({ source: 'molly-image', type: 'connect' }, '*')
  })

  const originalFetch = window.fetch.bind(window)
  window.fetch = async (input, init) => {
    const request = new Request(input, init)
    if (!/^https?:/.test(request.url)) return originalFetch(request)
    request.signal.throwIfAborted()
    const bytes = request.body ? new Uint8Array(await request.arrayBuffer()) : null
    request.signal.throwIfAborted()
    const id = ++sequence
    const cancel = () => {
      port.postMessage({ type: 'cancel', id })
      const error = new DOMException('请求已取消', 'AbortError')
      pending.get(id)?.reject(error)
      pending.delete(id)
      streams.get(id)?.error(error)
      streams.delete(id)
      streamCleanup.get(id)?.()
      streamCleanup.delete(id)
    }
    request.signal.addEventListener('abort', cancel, { once: true })
    streamCleanup.set(id, () => request.signal.removeEventListener('abort', cancel))
    try {
      const response = await new Promise<{ status: number, headers: Record<string, string> }>((resolve, reject) => {
        pending.set(id, { resolve, reject })
        port.postMessage({ type: 'fetch', id, url: request.url, method: request.method, headers: Object.fromEntries(request.headers), bytes })
      })
      const stream = new ReadableStream<Uint8Array>({
        start(controller) { streams.set(id, controller) },
        cancel() { port.postMessage({ type: 'cancel', id }); streams.delete(id); streamCleanup.get(id)?.(); streamCleanup.delete(id) },
      })
      // 204/205/304 不允许携带 Response body。
      return new Response([204, 205, 304].includes(response.status) ? null : stream, response)
    } catch (error) {
      request.signal.removeEventListener('abort', cancel)
      streamCleanup.delete(id)
      throw error
    }
  }
}

export function notifyMolly(type: 'ready' | 'settled' | 'load-error') {
  if (mollyEmbedded) port?.postMessage({ type })
}
