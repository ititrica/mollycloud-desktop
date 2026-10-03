import { asRecord, type KeyGroup } from './contracts';

export const providerNames: Record<string, string> = {
  openai:'OpenAI', anthropic:'Claude', gemini:'Gemini', grok:'Grok', deepseek:'DeepSeek',
  antigravity:'Antigravity', kimi:'Kimi', zhipu:'智谱', minimax:'MiniMax', composite:'多平台', opencode_go:'OpenCode', typesafe:'TypeSafe',
};
export function providerTone(platform: string): string { return platform in providerNames ? platform : 'other'; }
export function groupFromKey(item: Record<string, unknown>): KeyGroup | null {
  if (!item.group_id) return null;
  const group = asRecord(item.group);
  const amount = Number(group.rate_multiplier);
  return {id:String(item.group_id),name:String(item.group_name ?? group.name ?? `分组 ${item.group_id}`),platform:String(group.platform ?? ''),
    description:String(group.description ?? ''),subscription:group.subscription_type === 'subscription',
    rate:group.rate_multiplier != null && Number.isFinite(amount) && amount >= 0 ? amount : null,
    default_rate:group.rate_multiplier != null && Number.isFinite(amount) && amount >= 0 ? amount : null,custom_rate:false};
}
