export {};

import { SynchronizedRef, Struct, Queue, Effect } from "effect";

interface HudConfig {
  background_opacity: number;
  text_color: string;
  font_size_pt: number;
  min_font_size_pt: number;
  default_text: string;
}

declare global {
  interface Window {
    electronHUD: {
      getConfig: () => Promise<HudConfig>;
      onTextChanged: (callback: (text: string) => void) => void;
      moveWindow: (position: string) => void;
    };
  }
}

const subtitleBox = document.getElementById("subtitle-box") as HTMLDivElement;
const subtitleText = document.getElementById(
  "subtitle-text",
) as HTMLParagraphElement;

const POSITIONS = ["top", "center", "bottom"] as const;

interface RendererState {
  readonly cfg: HudConfig;
  readonly posIndex: 0 | 1 | 2;
}

function renderText(text: string, cfg: HudConfig): void {
  subtitleText.textContent = text;
  subtitleText.style.color = cfg.text_color;
  subtitleText.style.fontSize = `${cfg.font_size_pt}pt`;
  subtitleBox.style.background = text
    ? `rgba(10, 10, 10, ${cfg.background_opacity})`
    : "transparent";
}

const STARTUP_MESSAGES = [
  "Hello world, Hello Shiroe!",
  "エル・プサイ・コングルゥ EL PSY CONGROO",
];

const program = Effect.gen(function* () {
  const state = yield* SynchronizedRef.make<RendererState>({
    cfg: {
      background_opacity: 0.45,
      text_color: "#f5e642",
      font_size_pt: 24,
      min_font_size_pt: 0,
      default_text: "",
    },
    posIndex: 2,
  });

  const initialCfg = yield* Effect.promise(() =>
    window.electronHUD.getConfig(),
  );
  yield* SynchronizedRef.update(state, (s) =>
    Struct.evolve(s, { cfg: () => initialCfg }),
  );

  const msg = STARTUP_MESSAGES[Math.random() < 0.5 ? 0 : 1];
  renderText(msg, initialCfg);

  const textQueue = yield* Queue.unbounded<string>();
  window.electronHUD.onTextChanged((text) =>
    Queue.unsafeOffer(textQueue, text),
  );

  document.addEventListener("keydown", (e) => {
    if (e.key === "ArrowUp" || e.key === "ArrowDown") {
      SynchronizedRef.update(state, (s) =>
        Struct.evolve(s, {
          posIndex: (i) => {
            const next =
              e.key === "ArrowUp"
                ? (((i + 2) % 3) as 0 | 1 | 2)
                : (((i + 1) % 3) as 0 | 1 | 2);
            window.electronHUD.moveWindow(POSITIONS[next]);
            return next;
          },
        }),
      );
    }
  });

  yield* Effect.forever(
    Effect.gen(function* () {
      const text = yield* Queue.take(textQueue);
      const { cfg: currentCfg } = yield* SynchronizedRef.get(state);
      renderText(text, currentCfg);
    }),
  );
});

Effect.runPromise(program).catch((e) =>
  console.error("[HUD renderer] fatal:", e),
);
