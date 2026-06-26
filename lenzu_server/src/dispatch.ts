import { Effect } from "effect";
import { HudWindowTag } from "./services/hud-window";

// Re-export the UdpMessage type extracted from udp-socket
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
    switch (cmd.type) {
      case "message":
      case "plaintext":
        return yield* hudWindow.sendText(cmd.text);
      case "position":
        return yield* onPosition(cmd.pos);
      case "shutdown":
        yield* Effect.logInfo("[UDP] Received shutdown command, quitting...");
        yield* hudWindow.close();
        return yield* onShutdown;
    }
  });
}
