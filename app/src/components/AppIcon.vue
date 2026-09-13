<script setup lang="ts">
defineProps<{ name: "overview" | "subscription" | "key" | "usage" | "assistant" | "pet" | "refresh" | "logout" | "shield" | "copy" | "chevron" | "arrow" | "wallet" | "spend" | "code" | "token" | "request" | "spark" | "database" | "edit" | "upload" | "document" | "settings" | "power" | "tray" | "image" | "update" | "sidebar" }>();
</script>

<template>
  <svg :viewBox="name === 'settings' ? '0 0 128 128' : '0 0 24 24'" aria-hidden="true" class="app-icon">
    <template v-if="name === 'image'">
      <rect x="3" y="3" width="18" height="18" rx="3" /><circle cx="8" cy="8" r="1.5" /><path d="m3 17 5-5 4 4 4-6 5 7" />
    </template>
    <template v-else-if="name === 'sidebar'">
      <rect x="3" y="4" width="18" height="16" rx="3" /><path d="M9 4v16" />
    </template>
    <template v-else-if="name === 'update'">
      <path d="M20 6v5h-5M4 18v-5h5M6.1 8a7 7 0 0 1 11.7-1.7L20 11M4 13l2.2 4.7A7 7 0 0 0 18 16" />
      <path d="m12 7 3 3h-2v4h-2v-4H9l3-3Zm0 10-3-3h2v-4h2v4h2l-3 3Z" />
    </template>
    <template v-else-if="name === 'settings'">
      <g fill="currentColor" stroke="none">
        <path d="M64,39A25,25,0,1,0,89,64,25,25,0,0,0,64,39Zm0,44A19,19,0,1,1,83,64,19,19,0,0,1,64,83Z" />
        <path d="M121,48h-8.93a1,1,0,0,1-.94-.68,49.9,49.9,0,0,0-2-4.85,1,1,0,0,1,.18-1.15L115.62,35a7,7,0,0,0,0-9.9L102.89,12.38a7,7,0,0,0-9.9,0l-6.31,6.31a1,1,0,0,1-1.15.18,49.76,49.76,0,0,0-4.85-2,1,1,0,0,1-.68-.94V7a7,7,0,0,0-7-7H55a7,7,0,0,0-7,7v8.93a1,1,0,0,1-.68.94,49.9,49.9,0,0,0-4.85,2,1,1,0,0,1-1.15-.18L35,12.38a7,7,0,0,0-9.9,0L12.38,25.11a7,7,0,0,0,0,9.9l6.31,6.31a1,1,0,0,1,.18,1.15,49.76,49.76,0,0,0-2,4.85,1,1,0,0,1-.94.68H7a7,7,0,0,0-7,7V73a7,7,0,0,0,7,7h8.93a1,1,0,0,1,.94.68,49.9,49.9,0,0,0,2,4.85,1,1,0,0,1-.18,1.15L12.38,93a7,7,0,0,0,0,9.9l12.73,12.73a7,7,0,0,0,9.9,0l6.31-6.31a1,1,0,0,1,1.15-.18,49.76,49.76,0,0,0,4.85,2,1,1,0,0,1,.68.94V121a7,7,0,0,0,7,7H73a7,7,0,0,0,7-7v-8.93a1,1,0,0,1,.68-.94,49.9,49.9,0,0,0,4.85-2,1,1,0,0,1,1.15.18L93,115.62a7,7,0,0,0,9.9,0l12.73-12.73a7,7,0,0,0,0-9.9l-6.31-6.31a1,1,0,0,1-.18-1.15,49.76,49.76,0,0,0,2-4.85,1,1,0,0,1,.94-.68H121a7,7,0,0,0,7-7V55A7,7,0,0,0,121,48Zm1,25a1,1,0,0,1-1,1h-8.93a7,7,0,0,0-6.6,4.69,43.9,43.9,0,0,1-1.76,4.26,7,7,0,0,0,1.35,8l6.31,6.31a1,1,0,0,1,0,1.41L98.65,111.38a1,1,0,0,1-1.41,0l-6.31-6.31a7,7,0,0,0-8-1.35,43.88,43.88,0,0,1-4.27,1.76,7,7,0,0,0-4.68,6.6V121a1,1,0,0,1-1,1H55a1,1,0,0,1-1-1v-8.93a7,7,0,0,0-4.69-6.6,43.9,43.9,0,0,1-4.26-1.76,7,7,0,0,0-8,1.35l-6.31,6.31a1,1,0,0,1-1.41,0L16.62,98.65a1,1,0,0,1,0-1.41l6.31-6.31a7,7,0,0,0,1.35-8,43.88,43.88,0,0,1-1.76-4.27A7,7,0,0,0,15.93,74H7a1,1,0,0,1-1-1V55a1,1,0,0,1,1-1h8.93a7,7,0,0,0,6.6-4.69,43.9,43.9,0,0,1,1.76-4.26,7,7,0,0,0-1.35-8l-6.31-6.31a1,1,0,0,1,0-1.41L29.35,16.62a1,1,0,0,1,1.41,0l6.31,6.31a7,7,0,0,0,8,1.35,43.88,43.88,0,0,1,4.27-1.76A7,7,0,0,0,54,15.93V7a1,1,0,0,1,1-1H73a1,1,0,0,1,1,1v8.93a7,7,0,0,0,4.69,6.6,43.9,43.9,0,0,1,4.26,1.76,7,7,0,0,0,8-1.35l6.31-6.31a1,1,0,0,1,1.41,0l12.73,12.73a1,1,0,0,1,0,1.41l-6.31,6.31a7,7,0,0,0-1.35,8,43.88,43.88,0,0,1,1.76,4.27,7,7,0,0,0,6.6,4.68H121a1,1,0,0,1,1,1Z" />
      </g>
    </template>
    <template v-else-if="name === 'power'">
      <path d="M12 3v8m-5.7-5a8 8 0 1 0 11.4 0" />
    </template>
    <template v-else-if="name === 'tray'">
      <path d="M4 14v5a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-5M12 3v12m-4-4 4 4 4-4" />
    </template>
    <template v-else-if="name === 'overview'">
      <path d="M4 4h6v9H4V4Zm0 12h6v4H4v-4Zm10-5h6v9h-6v-9Zm0-7h6v4h-6V4Z" />
    </template>
    <template v-else-if="name === 'subscription'">
      <path d="M4 7h16M7 3v4m10-4v4M5 5h14a2 2 0 0 1 2 2v12a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V7a2 2 0 0 1 2-2Zm3 7h8m-8 4h5" />
    </template>
    <template v-else-if="name === 'key'">
      <path d="M15 7a5 5 0 1 0-3.7 8.4L14 18h3v-3h3v-3l-2-2m-8 1h.01" />
    </template>
    <template v-else-if="name === 'usage'">
      <path d="M4 19V9m5 10V5m5 14v-7m5 7V3" />
    </template>
    <template v-else-if="name === 'assistant'">
      <path d="M6 4h12a3 3 0 0 1 3 3v7a3 3 0 0 1-3 3h-6l-4 3v-3H6a3 3 0 0 1-3-3V7a3 3 0 0 1 3-3Zm3 6h.01M12 10h.01M15 10h.01" />
    </template>
    <template v-else-if="name === 'pet'">
      <path d="M8 7 5 4 3 10a9 9 0 0 0 18 0l-2-6-3 3M8 13h.01M16 13h.01M9 17c2 1 4 1 6 0" />
    </template>
    <template v-else-if="name === 'refresh'">
      <path d="M20 6v5h-5M4 18v-5h5M6.1 8a7 7 0 0 1 11.7-1.7L20 11M4 13l2.2 4.7A7 7 0 0 0 18 16" />
    </template>
    <template v-else-if="name === 'logout'">
      <path d="M10 5H5a2 2 0 0 0-2 2v10a2 2 0 0 0 2 2h5m5-4 4-3-4-3m4 3H8" />
    </template>
    <template v-else-if="name === 'copy'">
      <path d="M9 8h9a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H9a2 2 0 0 1-2-2v-9a2 2 0 0 1 2-2Zm-5 8H3V5a2 2 0 0 1 2-2h11v1" />
    </template>
    <template v-else-if="name === 'chevron'">
      <path d="m8 10 4 4 4-4" />
    </template>
    <template v-else-if="name === 'arrow'">
      <path d="m9 5 7 7-7 7" />
    </template>
    <template v-else-if="name === 'wallet'">
      <path d="M4 7.5h14a2 2 0 0 1 2 2V18a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h11v3.5m1 4H20v4h-3a2 2 0 0 1 0-4Z" />
    </template>
    <template v-else-if="name === 'spend'">
      <path d="M4 7h16v11a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V7Zm0 4h16M8 4h8" />
    </template>
    <template v-else-if="name === 'code'">
      <path d="m8 8-4 4 4 4m8-8 4 4-4 4m-2-11-4 14" />
    </template>
    <template v-else-if="name === 'token'">
      <path d="M5 20V10m4 10V5m4 15v-7m4 7V8m3 12H3" />
    </template>
    <template v-else-if="name === 'request'">
      <path d="M5 19V9m5 10v-5m5 5V6m4 13V3M3 19h18" />
    </template>
    <template v-else-if="name === 'database'">
      <ellipse cx="12" cy="5" rx="7" ry="3" />
      <path d="M5 5v14c0 1.7 3.1 3 7 3s7-1.3 7-3V5M5 10c0 1.7 3.1 3 7 3s7-1.3 7-3M5 15c0 1.7 3.1 3 7 3s7-1.3 7-3" />
    </template>
    <template v-else-if="name === 'edit'">
      <path d="M12 4H5a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2h13a2 2 0 0 0 2-2v-7M15.5 3.5a2.1 2.1 0 0 1 3 3L11 14l-4 1 1-4 7.5-7.5ZM14 5l3 3" />
    </template>
    <template v-else-if="name === 'upload'">
      <path d="M12 15V3m-4 4 4-4 4 4M4 12v7a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-7" />
    </template>
    <template v-else-if="name === 'document'">
      <path d="M6 3h12a2 2 0 0 1 2 2v15a1 1 0 0 1-1 1H6a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2Zm2 5h8m-8 4h8m-8 4h5" />
    </template>
    <template v-else-if="name === 'spark'">
      <path d="m12 3 1.6 4.4L18 9l-4.4 1.6L12 15l-1.6-4.4L6 9l4.4-1.6L12 3Zm6 11 .8 2.2L21 17l-2.2.8L18 20l-.8-2.2L15 17l2.2-.8L18 14Z" />
    </template>
    <template v-else>
      <path d="m12 3 8 4v5c0 5-3.4 8-8 9-4.6-1-8-4-8-9V7l8-4Zm-3 9 2 2 4-5" />
    </template>
  </svg>
</template>
