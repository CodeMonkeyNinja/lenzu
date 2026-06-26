export {};

import { SynchronizedRef, Queue, Effect, pipe, Option, Array } from "effect";

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

function renderText(text: string, cfg: HudConfig): void {
  subtitleText.textContent = text;
  subtitleText.style.color = cfg.text_color;
  subtitleText.style.fontSize = `${cfg.font_size_pt}pt`;
  subtitleBox.style.background = pipe(
    Option.fromNullable(text || null),
    Option.match({
      onNone: () => "transparent",
      onSome: () => `rgba(10, 10, 10, ${cfg.background_opacity})`,
    }),
  );
}

const STARTUP_MESSAGES = [
  "Hello world, Hello Shiroe!",
  "エル・プサイ・コングルゥ EL PSY CONGROO",
];

const DEFAULT_CONFIG: HudConfig = {
  background_opacity: 0.45,
  text_color: "#f5e642",
  font_size_pt: 24,
  min_font_size_pt: 0,
  default_text: "",
};

const DEFAULT_MSG = "Hello world";

const program = Effect.gen(function* () {
  const cfgRef = yield* SynchronizedRef.make<HudConfig>(DEFAULT_CONFIG);

  const initialCfg = yield* Effect.promise(() =>
    window.electronHUD.getConfig(),
  );
  yield* SynchronizedRef.set(cfgRef, initialCfg);

  const msg = pipe(
    Array.get(STARTUP_MESSAGES, Math.random() < 0.5 ? 0 : 1),
    Option.getOrElse(() => DEFAULT_MSG),
  );
  renderText(msg, initialCfg);

  const textQueue = yield* Queue.unbounded<string>();
  window.electronHUD.onTextChanged((text) =>
    Queue.unsafeOffer(textQueue, text),
  );

  let posIndex: 0 | 1 | 2 = 2;

  document.addEventListener("keydown", (e) => {
    if (e.key === "ArrowUp" || e.key === "ArrowDown") {
      const step = e.key === "ArrowUp" ? 2 : 1;
      posIndex = pipe(
        posIndex,
        (i) => (((i + step) % 3) + 3) % 3,
        (i) => i as 0 | 1 | 2,
      );
      window.electronHUD.moveWindow(POSITIONS[posIndex]);
    }
  });

  yield* Effect.forever(
    Effect.gen(function* () {
      const text = yield* Queue.take(textQueue);
      const currentCfg = yield* SynchronizedRef.get(cfgRef);
      renderText(text, currentCfg);
    }),
  );
});

Effect.runPromise(program).catch((e) =>
  console.error("[HUD renderer] fatal:", e),
);
