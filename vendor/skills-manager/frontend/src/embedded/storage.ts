import { native } from "./bridge";
const prefix = () => native() ? "mollycloud:skills:" : "mollycloud:preview:skills:";
export const skillStorage = {
  getItem(key: string) { try { return localStorage.getItem(prefix() + key); } catch { return null; } },
  setItem(key: string, value: string) { localStorage.setItem(prefix() + key, value); },
  removeItem(key: string) { localStorage.removeItem(prefix() + key); },
};
