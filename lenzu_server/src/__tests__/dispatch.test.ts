import { describe, it, expect } from "vitest";
import { Effect } from "effect";
import { processMessage, type UdpMessage } from "../dispatch";
import { HudWindowTest } from "../services/hud-window";

describe("processMessage", () => {
  it("sends text to HudWindow for message type", () => {
    const testWindow = HudWindowTest();
    const cmd: UdpMessage = { type: "message", text: "hello" };

    Effect.runSync(
      Effect.provide(
        processMessage(cmd, Effect.void, Effect.void),
        testWindow.layer,
      ),
    );

    expect(testWindow.sent).toEqual(["hello"]);
  });

  it("sends text to HudWindow for plaintext type", () => {
    const testWindow = HudWindowTest();
    const cmd: UdpMessage = { type: "plaintext", text: "world" };

    Effect.runSync(
      Effect.provide(
        processMessage(cmd, Effect.void, Effect.void),
        testWindow.layer,
      ),
    );

    expect(testWindow.sent).toEqual(["world"]);
  });

  it("calls onPosition callback for position type", () => {
    const testWindow = HudWindowTest();
    const positions: string[] = [];
    const cmd: UdpMessage = { type: "position", pos: "top" };

    Effect.runSync(
      Effect.provide(
        processMessage(cmd, Effect.void, (pos) =>
          Effect.sync(() => {
            positions.push(pos);
          }),
        ),
        testWindow.layer,
      ),
    );

    expect(positions).toEqual(["top"]);
  });

  it("closes window and calls onShutdown callback for shutdown type", () => {
    const testWindow = HudWindowTest();
    let shutDown = false;
    const cmd: UdpMessage = { type: "shutdown" };

    Effect.runSync(
      Effect.provide(
        processMessage(
          cmd,
          Effect.sync(() => {
            shutDown = true;
          }),
          Effect.void,
        ),
        testWindow.layer,
      ),
    );

    expect(shutDown).toBe(true);
  });
});
