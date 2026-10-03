/// <reference types="vite/client" />

declare const __MOLLY_PLATFORM__: 'macos' | 'windows' | 'linux';

declare module "*.vue" {
  import type { DefineComponent } from "vue";
  const component: DefineComponent<{}, {}, any>;
  export default component;
}
