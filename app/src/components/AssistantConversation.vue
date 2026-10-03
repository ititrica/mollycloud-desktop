<script setup lang="ts">
import { nextTick, onActivated, onMounted, ref, watch } from 'vue';
import { NAlert, NButton, NCard, NInput } from 'naive-ui';
import type { SpeechStatus } from '../speech';
const props=defineProps<{messages:Array<{role:'user'|'assistant';content:string}>;username:string;draft:string;sending:boolean;error:string;speechStatus:SpeechStatus}>();
const emit=defineEmits<{'update:draft':[string];send:[];stop:[];keydown:[KeyboardEvent]}>();
const history=ref<HTMLElement|null>(null);
async function scrollToEnd(){await nextTick();if(history.value)history.value.scrollTop=history.value.scrollHeight;}
watch(()=>[props.messages.length,props.sending],()=>void scrollToEnd());onMounted(scrollToEnd);onActivated(scrollToEnd);defineExpose({scrollToEnd});
</script>
<template>
  <n-card class="panel assistant-chat" :bordered="false">
          <div ref="history" class="assistant-chat__history" aria-live="polite" :aria-busy="sending">
            <div v-if="messages.length === 0" class="assistant-chat__empty">
              <div class="assistant-avatar assistant-avatar--molly"><img src="/brand/molly.png" alt="Molly" /></div>
              <strong>还没有对话</strong>
              <p>和 Molly 打个招呼吧，这里的对话会与桌面 Molly 同步。</p>
            </div>
            <article v-for="(message, index) in messages" :key="`${index}-${message.role}`" class="assistant-message" :class="`assistant-message--${message.role}`">
              <div v-if="message.role === 'assistant'" class="assistant-avatar assistant-avatar--molly"><img src="/brand/molly.png" alt="Molly" /></div>
              <div class="assistant-message__body">
                <span>{{ message.role === 'assistant' ? 'Molly' : username }}</span>
                <p>{{ message.content }}</p>
              </div>
              <div v-if="message.role === 'user'" class="assistant-avatar assistant-avatar--user" :aria-label="username">{{ username.trim().charAt(0).toUpperCase() || '我' }}</div>
            </article>
            <article v-if="sending && messages[messages.length - 1]?.role !== 'assistant'" class="assistant-message assistant-message--assistant">
              <div class="assistant-avatar assistant-avatar--molly"><img src="/brand/molly.png" alt="Molly" /></div>
              <div class="assistant-message__body assistant-message__body--typing"><span>Molly</span><p><i /><i /><i /></p></div>
            </article>
          </div>

          <div class="assistant-composer">
            <div v-if="speechStatus.phase !== 'idle'" class="assistant-speech-status" :data-error="speechStatus.phase === 'error'"><span role="status">{{ speechStatus.phase === 'preparing' ? '正在准备语音…' : speechStatus.phase === 'playing' ? 'Molly 正在朗读…' : `语音播放失败：${speechStatus.error ?? '请检查语音设置'}` }}</span><n-button text size="small" @click="emit('stop')">{{ speechStatus.phase === 'error' ? '关闭提示' : '停止朗读' }}</n-button></div>
            <n-alert v-if="error" class="assistant-chat__error" type="error" :show-icon="false">{{ error }}</n-alert>
            <div class="assistant-composer__row">
              <n-input :value="draft" @update:value="emit('update:draft', $event)" class="assistant-chat__input" size="large" maxlength="2000" :disabled="sending" :input-props="{ 'aria-label': '给 Molly 发送消息' }" placeholder="给 Molly 发送消息…" @keydown="emit('keydown', $event)" />
              <div class="assistant-composer__actions">
                <n-button class="assistant-send-button" type="primary" :loading="sending" :disabled="!draft.trim() || sending" @click="emit('send')">发送</n-button>
              </div>
            </div>
          </div>
  </n-card>
</template>
