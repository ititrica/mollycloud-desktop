import { Channel, invoke } from '@tauri-apps/api/core';

const stores = ['tasks', 'images', 'thumbnails', 'agentConversations'];
const methods = ['get', 'getAll', 'getAllKeys', 'put', 'delete', 'clear'];
type Operation = { store: string; method: string; value?: unknown };

export async function imageDatabase(name: string, operations: Operation[]): Promise<unknown> {
  if (!Array.isArray(operations) || operations.some(op => !stores.includes(op.store) || !methods.includes(op.method))) throw new Error('无效的工作台存储操作。');
  if (!operations.length) return;
  const db = await new Promise<IDBDatabase>((resolve, reject) => {
    const request = indexedDB.open(name, 3);
    request.onupgradeneeded = () => {
      for (const store of stores) if (!request.result.objectStoreNames.contains(store)) request.result.createObjectStore(store, { keyPath: 'id' });
    };
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error);
    request.onblocked = () => reject(new Error('作品库被其他窗口占用，请关闭重复窗口后重试。'));
  });
  try {
    return await new Promise((resolve, reject) => {
      const write = operations.some(op => ['put', 'delete', 'clear'].includes(op.method));
      const tx = db.transaction([...new Set(operations.map(op => op.store))], write ? 'readwrite' : 'readonly');
      let result: unknown;
      for (const op of operations) {
        const store = tx.objectStore(op.store);
        let request: IDBRequest;
        switch (op.method) {
          case 'get': request = store.get(op.value as IDBValidKey); break;
          case 'getAll': request = store.getAll(); break;
          case 'getAllKeys': request = store.getAllKeys(); break;
          case 'put': request = store.put(op.value); break;
          case 'delete': request = store.delete(op.value as IDBValidKey); break;
          default: request = store.clear();
        }
        request.onsuccess = () => { result = request.result; };
      }
      tx.oncomplete = () => resolve(result);
      tx.onerror = tx.onabort = () => reject(tx.error ?? new Error('作品保存失败。'));
    });
  } finally { db.close(); }
}

type NetworkRequest = { url: string; method: string; headers: Record<string, string>; bytes?: Uint8Array | null };
export function validateImageRequest(request: NetworkRequest): URL {
  const url = new URL(request.url);
  const loopback = ['localhost', '127.0.0.1', '[::1]'].includes(url.hostname);
  if ((url.protocol !== 'https:' && !(url.protocol === 'http:' && loopback)) || url.username || url.password || url.hostname.endsWith('.localhost')) throw new Error('请求地址需要使用 HTTPS；本地服务可使用 HTTP。');
  if (!['GET', 'HEAD', 'POST', 'PUT', 'PATCH', 'DELETE'].includes(request.method)) throw new Error('不支持此请求方法。');
  if (request.bytes && request.bytes.byteLength > 128 * 1024 * 1024) throw new Error('上传内容超过 128 MB。');
  return url;
}

export async function forwardImageRequest(request: NetworkRequest, id: string, send: (data: object) => void, signal: AbortSignal): Promise<void> {
  validateImageRequest(request);
  signal.throwIfAborted();
  if ('__TAURI_INTERNALS__' in window) {
    const channel = new Channel<{ type: string; status?: number; headers?: Record<string, string>; data?: string }>();
    channel.onmessage = (data) => {
      if (signal.aborted) return;
      if (data.type === 'headers') send({ value: { status: data.status, headers: data.headers } });
      if (data.type === 'chunk') send({ type: 'chunk', bytes: Uint8Array.from(atob(data.data!), char => char.charCodeAt(0)) });
    };
    const cancel = () => { void invoke('cancel_image_request', { id }).catch(() => undefined); };
    signal.addEventListener('abort', cancel, { once: true });
    let body: string | null = null;
    if (request.bytes) {
      const parts: string[] = [];
      for (let i = 0; i < request.bytes.length; i += 32768) parts.push(String.fromCharCode(...request.bytes.subarray(i, i + 32768)));
      body = btoa(parts.join(''));
    }
    try {
      await invoke('image_request', { id, request: { url: request.url, method: request.method, headers: request.headers, body }, channel });
      send({ type: 'end' });
    } finally { signal.removeEventListener('abort', cancel); }
    return;
  }
  // 浏览器预览直接请求；桌面版使用上面的流式转发。
  const response = await fetch(request.url, { method: request.method, headers: request.headers, body: request.bytes ? new Uint8Array(request.bytes).buffer : undefined, signal, credentials: 'omit' });
  send({ value: { status: response.status, headers: Object.fromEntries(response.headers) } });
  const reader = response.body?.getReader();
  if (reader) {
    while (true) {
      const result = await reader.read();
      if (result.done) break;
      send({ type: 'chunk', bytes: result.value });
    }
  }
  send({ type: 'end' });
}
