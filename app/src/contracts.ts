import { z } from "zod";

const unknownRecord = z.record(z.string(), z.unknown());

export const serviceBootstrapSchema = z.object({
  service_origin: z.string().url(),
  health: unknownRecord,
  settings: unknownRecord,
});

export const loginOutcomeSchema = z.object({
  requires_two_factor: z.boolean(),
  user_email_masked: z.string().nullable().optional(),
  user: z.unknown(),
});

export const dashboardPayloadSchema = z.object({
  user: z.unknown(),
  subscriptions: z.unknown(),
  subscription_progress: z.unknown(),
  usage: z.unknown(),
  keys: z.unknown(),
});

export const assistantKeyOptionSchema = z.object({
  id: z.string(),
  name: z.string(),
  masked_key: z.string(),
});

export const assistantStatusSchema = z.object({
  ready: z.boolean(),
  keys: z.array(assistantKeyOptionSchema),
  selected_key_id: z.string().nullable(),
  models: z.array(z.string()),
  message: z.string().nullable(),
});

export const assistantReplySchema = z.object({
  content: z.string(),
  tools_used: z.array(z.string()),
});

export const assistantConfigSchema = z.object({
  enabled: z.boolean(),
  provider: z.string(),
  model: z.string(),
  persona: z.string(),
  custom_base_url: z.string(),
  greet_interval: z.number(),
  molly_key_id: z.string(),
  api_key_configured: z.boolean(),
});

export type ServiceBootstrap = z.infer<typeof serviceBootstrapSchema>;
export type LoginOutcome = z.infer<typeof loginOutcomeSchema>;
export type DashboardPayload = z.infer<typeof dashboardPayloadSchema>;
export type AssistantStatus = z.infer<typeof assistantStatusSchema>;
export type AssistantReply = z.infer<typeof assistantReplySchema>;
export type AssistantConfig = z.infer<typeof assistantConfigSchema>;

export interface AssistantConfigUpdate {
  enabled: boolean;
  provider: string;
  model: string;
  persona: string;
  custom_base_url: string;
  greet_interval: number;
  molly_key_id: string;
  api_key?: string | null;
  clear_api_key?: boolean;
}

export interface AssistantMessage {
  role: "user" | "assistant";
  content: string;
}

export type JsonRecord = Record<string, unknown>;

export function asRecord(value: unknown): JsonRecord {
  return value && typeof value === "object" && !Array.isArray(value)
    ? (value as JsonRecord)
    : {};
}

export function asArray(value: unknown): JsonRecord[] {
  return Array.isArray(value) ? value.map(asRecord) : [];
}
