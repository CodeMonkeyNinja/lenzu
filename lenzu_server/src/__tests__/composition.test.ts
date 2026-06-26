import { describe, it, expect } from "vitest";
import { Effect, Layer, Queue } from "effect";
import { HudConfigService, HudConfigTest } from "../services/hud-config";
import { UdpSocketTag, UdpSocketTest } from "../services/udp-socket";
import { HudWindowTag, HudWindowTest } from "../services/hud-window";
import { processMessage, type UdpMessage } from "../dispatch";

describe("Layer composition", () => {
  it("provides HudConfigService and UdpSocketTag via Layer.provideMerge", () => {
    const testLayer = Layer.provideMerge(
      UdpSocketTest([{ type: "message", text: "hi" }]),
      HudConfigTest({ font_size_pt: 30 }),
    );

    const program = Effect.gen(function* () {
      const cfg = yield* HudConfigService;
      const { queue } = yield* UdpSocketTag;
      const msg = yield* Queue.take(queue);
      return { font_size_pt: cfg.font_size_pt, msg };
    });

    const result = Effect.runSync(program.pipe(Effect.provide(testLayer)));

    expect(result.font_size_pt).toBe(30);
    expect(result.msg).toEqual({ type: "message", text: "hi" });
  });
});

describe("processMessage with test layers", () => {
  it("sends text through HudWindowTest when HudWindowTag is provided", () => {
    const testWindow = HudWindowTest();

    const program = processMessage(
      { type: "message", text: "hello" },
      Effect.void,
      Effect.void,
    );

    Effect.runSync(program.pipe(Effect.provide(testWindow.layer)));

    expect(testWindow.sent).toEqual(["hello"]);
  });

  it("handles shutdown through HudWindowTest", () => {
    const testWindow = HudWindowTest();
    let shutDown = false;

    const program = processMessage(
      { type: "shutdown" },
      Effect.sync(() => {
        shutDown = true;
      }),
      Effect.void,
    );

    Effect.runSync(program.pipe(Effect.provide(testWindow.layer)));

    expect(shutDown).toBe(true);
  });
});
