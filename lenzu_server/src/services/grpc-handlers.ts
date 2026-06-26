import { BrowserWindow, app, screen } from "electron";
import { Effect, Layer } from "effect";
import { processMessage } from "../dispatch";
import { HudWindowTag, HudWindowLive } from "./hud-window";
import { computePosition, type WindowPosition } from "../window-position";
import type { HudConfig } from "../config";

/// Process a SendText RPC: forward to dispatch pipeline via Effect.
/// Accepts a HudWindow layer for testability (pass HudWindowTest().layer in tests).
export function handleSendText(
  text: string,
  hudWindowLayer: Layer.Layer<HudWindowTag>,
): Promise<void> {
  const cmd = { type: "message" as const, text };
  const handleShutdown = Effect.sync(() => app.quit());
  const handlePosition = (_pos: "top" | "bottom") => Effect.void;
  return Effect.runPromise(
    processMessage(cmd, handleShutdown, handlePosition).pipe(
      Effect.provide(hudWindowLayer),
    ),
  );
}

/// Process a MoveWindow RPC: reposition the HUD overlay.
export function handleMoveWindow(
  pos: WindowPosition,
  mainWindow: BrowserWindow,
  config: HudConfig,
): void {
  if (!mainWindow || mainWindow.isDestroyed()) return;
  const cursor = screen.getCursorScreenPoint();
  const display = screen.getDisplayNearestPoint(cursor);
  const winSize = mainWindow.getSize() as [number, number];
  const { x, y } = computePosition(
    display.workArea,
    winSize,
    pos,
    config.bottom_margin,
  );
  mainWindow.setPosition(x, y);
}

/// Process a Shutdown RPC: quit the app.
export function handleShutdown(): void {
  app.quit();
}

/// Shorthand: create a HudWindowLive layer from a BrowserWindow.
export function liveHudLayer(win: BrowserWindow): Layer.Layer<HudWindowTag> {
  return HudWindowLive(win);
}
