// Upstream frontendLogger redacts credentials before calling this adapter.
export async function error(message: string, _options?: unknown): Promise<void> {
  console.error("[Molly CC Switch]", message);
}
