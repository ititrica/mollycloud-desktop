<script setup lang="ts">
import type { KeyGroup } from '../contracts';
import { providerTone } from '../keyGroups';
import ProviderIcon from './ProviderIcon.vue';
defineProps<{ group: KeyGroup; showRate?: boolean }>();
</script>
<template>
  <span class="provider-badge" :data-provider="providerTone(group.platform)" :class="{'provider-badge--subscription':group.subscription}" :title="group.name">
    <ProviderIcon :platform="group.platform" /><strong>{{ group.name }}</strong>
    <span v-if="showRate && group.rate !== null" class="provider-badge__rate"><s v-if="group.custom_rate && group.default_rate !== null && group.default_rate !== group.rate">{{ group.default_rate }}x</s>{{ group.rate }}x</span>
  </span>
</template>
