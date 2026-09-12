import { ShaderSystem } from "@pixi/core";
import { install } from "@pixi/unsafe-eval";
import { Application, Ticker } from "pixi.js";
import { Live2DModel } from "pixi-live2d-display/cubism4";

install({ ShaderSystem });
Live2DModel.registerTicker(Ticker);

export type MotionGroup = "Idle" | "Blink" | "Nod" | "Shake";

export interface MollyRenderer {
  play(group: MotionGroup): Promise<void>;
  focus(clientX: number, clientY: number): void;
  resize(): void;
  destroy(): void;
}

export async function createMollyRenderer(canvas: HTMLCanvasElement): Promise<MollyRenderer> {
  const parent = canvas.parentElement;
  if (!parent) throw new Error("模型画布尚未准备好");
  const container = parent;

  const app = new Application({
    view: canvas,
    width: Math.max(container.clientWidth, 1),
    height: Math.max(container.clientHeight, 1),
    backgroundAlpha: 0,
    antialias: true,
    autoDensity: true,
    resolution: Math.min(window.devicePixelRatio || 1, 2),
    sharedTicker: true,
  } as unknown as ConstructorParameters<typeof Application>[0]);

  const model = await Promise.race([
    Live2DModel.from("/live2d/molly/seethrough_output.model3.json", { autoInteract: false }),
    new Promise<never>((_, reject) => window.setTimeout(() => reject(new Error("模型加载超时")), 20_000)),
  ]);
  model.anchor.set(0.5, 1);
  app.stage.addChild(model);

  function resize(): void {
    const width = Math.max(container.clientWidth, 1);
    const height = Math.max(container.clientHeight, 1);
    app.renderer.resize(width, height);
    const naturalWidth = Number(model.width) / Math.max(Number(model.scale.x), 0.0001);
    const naturalHeight = Number(model.height) / Math.max(Number(model.scale.y), 0.0001);
    const scale = Math.min((width * 0.96) / naturalWidth, (height * 0.98) / naturalHeight);
    model.scale.set(scale);
    model.x = width / 2;
    model.y = height + 2;
  }

  resize();
  const observer = new ResizeObserver(resize);
  observer.observe(container);
  await model.motion("Idle", 0);

  return {
    async play(group) {
      await model.motion(group, 0);
    },
    focus(clientX, clientY) {
      model.focus(clientX, clientY);
    },
    resize,
    destroy() {
      observer.disconnect();
      app.destroy(true, { children: true, texture: true, baseTexture: true });
    },
  };
}
