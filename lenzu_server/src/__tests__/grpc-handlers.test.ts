import { describe, it, expect } from "vitest";
import { Effect } from "effect";
import { handleSendText } from "../services/grpc-handlers";
import { HudWindowTest } from "../services/hud-window";

describe("handleSendText", () => {
  it("sends text to HudWindow", async () => {
    const testWindow = HudWindowTest();
    await handleSendText("hello gRPC", testWindow.layer);
    expect(testWindow.sent).toEqual(["hello gRPC"]);
  });

  it("sends empty string to HudWindow", async () => {
    const testWindow = HudWindowTest();
    await handleSendText("", testWindow.layer);
    expect(testWindow.sent).toEqual([""]);
  });

  it("sends multiple texts sequentially", async () => {
    const testWindow = HudWindowTest();
    await handleSendText("first", testWindow.layer);
    await handleSendText("second", testWindow.layer);
    expect(testWindow.sent).toEqual(["first", "second"]);
  });
});
