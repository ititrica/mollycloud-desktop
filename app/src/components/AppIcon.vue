<script setup lang="ts">
defineProps<{ name: "overview" | "subscription" | "key" | "usage" | "assistant" | "pet" | "refresh" | "logout" | "shield" | "copy" | "chevron" | "arrow" | "wallet" | "spend" | "code" | "token" | "request" | "spark" | "database" | "edit" | "upload" | "document" | "settings" | "power" | "tray" | "image" }>();
</script>

<template>
  <svg viewBox="0 0 24 24" aria-hidden="true" class="app-icon">
    <template v-if="name === 'image'">
      <rect x="3" y="3" width="18" height="18" rx="3" /><circle cx="8" cy="8" r="1.5" /><path d="m3 17 5-5 4 4 4-6 5 7" />
    </template>
    <template v-else-if="name === 'settings'">
      <path d="m9.5 4 .7-2h3.6l.7 2 2 .9 2.1-.4 1.8 3.1-1.4 1.6.2 2.2 1.4 1.6-1.8 3.1-2.1-.4-2 .9-.7 2h-3.6l-.7-2-2-.9-2.1.4-1.8-3.1L5.1 13l-.2-2.2-1.4-1.6 1.8-3.1 2.1.4 2.1-1.2Z" transform="translate(0 1)" /><circle cx="12" cy="12" r="3" />
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
