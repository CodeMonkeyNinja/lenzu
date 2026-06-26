import { Context, Layer, Effect } from "effect";
import { BrowserWindow } from "electron";

export interface HudWindowService {
  readonly sendText: (text: string) => Effect.Effect<void>;
  readonly sendPosition: (pos: string) => Effect.Effect<void>;
  readonly close: () => Effect.Effect<void>;
}

export class HudWindowTag extends Context.Tag("HudWindow")<
  HudWindowTag,
  HudWindowService
>() {}

export const HudWindowLive = (win: BrowserWindow) =>
  Layer.succeed(HudWindowTag, {
    sendText: (t) =>
      Effect.sync(() => win.webContents.send("hud-text-changed", t)),
    sendPosition: (pos) =>
      Effect.sync(() => win.webContents.send("hud-position-changed", pos)),
    close: () => Effect.sync(() => win.close()),
  });

export const HudWindowTest = () => {
  const sent: string[] = [];
  return {
    layer: Layer.succeed(HudWindowTag, {
      sendText: (t) =>
        Effect.sync(() => {
          sent.push(t);
        }),
      sendPosition: (pos) =>
        Effect.sync(() => {
          sent.push(`position:${pos}`);
        }),
      close: () => Effect.void,
    }),
    sent,
  };
};
