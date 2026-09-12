import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { UpdateProvider } from "./contexts/UpdateContext";
import "./index.css";
import "./embedded/embedded.css";
import "./i18n";
import { QueryClientProvider } from "@tanstack/react-query";
import { ThemeProvider } from "@/components/theme-provider";
import { queryClient } from "@/lib/query";
import { Toaster } from "@/components/ui/sonner";
import { invoke } from "@tauri-apps/api/core";
import { FrontendErrorBoundary } from "./components/FrontendErrorBoundary";
import { installGlobalErrorHandlers } from "./lib/frontendLogger";
import { initializeWindowActivity } from "@/lib/windowActivity";
import { reportLoadError } from "@/embedded/bridge";

installGlobalErrorHandlers();

async function bootstrap() {
  const root = ReactDOM.createRoot(document.getElementById("root")!);
  try {
    const initError = await invoke<{ error?: string; path?: string } | null>("get_init_error");
    if (initError) throw new Error(initError.error ?? "私有配置初始化失败");
  } catch (error) {
    root.render(<div role="alert" className="p-8 text-sm"><h1 className="mb-3 font-semibold">无法打开内置 CC Switch</h1><p>{error instanceof Error ? error.message : String(error)}</p><p className="mt-3 text-muted-foreground">MollyCloud 的其他功能仍可使用。请修复私有配置，或随 MollyCloud 更新内置模块。</p></div>);
    reportLoadError();
    return;
  }
  initializeWindowActivity();
  root.render(
    <React.StrictMode>
      <FrontendErrorBoundary>
        <QueryClientProvider client={queryClient}>
          <ThemeProvider defaultTheme="light" storageKey="cc-switch-theme">
            <UpdateProvider><App /><Toaster /></UpdateProvider>
          </ThemeProvider>
        </QueryClientProvider>
      </FrontendErrorBoundary>
    </React.StrictMode>,
  );
}
void bootstrap();
