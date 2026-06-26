import { Effect, Match } from "effect";
import { HudWindowTag } from "./services/hud-window";

export type UdpMessage =
  | { readonly type: "message"; readonly text: string }
  | { readonly type: "plaintext"; readonly text: string }
  | { readonly type: "position"; readonly pos: "top" | "bottom" }
  | { readonly type: "shutdown" };

export function processMessage(
  cmd: UdpMessage,
  onShutdown: Effect.Effect<void>,
  onPosition: (pos: "top" | "bottom") => Effect.Effect<void>,
): Effect.Effect<void> {
  return Effect.gen(function* () {
    const hudWindow = yield* HudWindowTag;
    return yield* Match.value(cmd).pipe(
      Match.when({ type: "message" }, (c) => hudWindow.sendText(c.text)),
      Match.when({ type: "plaintext" }, (c) => hudWindow.sendText(c.text)),
      Match.when({ type: "position" }, (c) => onPosition(c.pos)),
      Match.when({ type: "shutdown" }, () =>
        Effect.logInfo("[UDP] Received shutdown command, quitting...").pipe(
          Effect.andThen(hudWindow.close()),
          Effect.andThen(onShutdown),
        ),
      ),
      Match.exhaustive,
    );
  });
}
