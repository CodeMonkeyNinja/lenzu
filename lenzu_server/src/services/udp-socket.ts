import { Context, Layer, Effect, Queue } from "effect";
import * as dgram from "dgram";
import { Schema } from "effect";
import { HudConfigService } from "./hud-config.js";

const UdpMessageSchema = Schema.Union(
  Schema.Struct({ type: Schema.Literal("message"), text: Schema.String }),
  Schema.Struct({ type: Schema.Literal("shutdown") }),
  Schema.Struct({
    type: Schema.Literal("position"),
    pos: Schema.Union(Schema.Literal("top"), Schema.Literal("bottom")),
  }),
  Schema.Struct({ type: Schema.Literal("plaintext"), text: Schema.String }),
);
type UdpMessage = Schema.Schema.Type<typeof UdpMessageSchema>;

function decodeUdpMessage(raw: string): UdpMessage {
  try {
    const parsed = JSON.parse(raw);
    const result = Schema.decodeUnknownSync(UdpMessageSchema, {
      onExcessProperty: "ignore",
    })(parsed);
    return result;
  } catch {
    return { type: "plaintext", text: raw };
  }
}

export interface UdpSocketService {
  readonly queue: Queue.Queue<UdpMessage>;
}

export class UdpSocketTag extends Context.Tag("UdpSocket")<
  UdpSocketTag,
  UdpSocketService
>() {}

export const UdpSocketLive: Layer.Layer<UdpSocketTag, never, HudConfigService> =
  Layer.effect(
    UdpSocketTag,
    Effect.gen(function* () {
      const config = yield* HudConfigService;
      const queue = yield* Queue.unbounded<UdpMessage>();
      const socket = dgram.createSocket("udp4");

      socket.on("message", (msg) => {
        const raw = msg.toString().trim();
        if (!raw) return;
        console.log(
          "[UDP] Received:",
          raw.substring(0, 200) + (raw.length > 200 ? "..." : ""),
        );
        Queue.unsafeOffer(queue, decodeUdpMessage(raw));
      });

      socket.on("error", (err) => {
        console.error("UDP error:", err);
        socket.close();
      });

      yield* Effect.async<void>((resume) => {
        socket.bind(config.udp_port, "127.0.0.1", () => {
          const addr = socket.address();
          console.log(`HUD listening on UDP ${addr.address}:${addr.port}`);
          resume(Effect.void);
        });
      });

      return { queue };
    }),
  );

export const UdpSocketTest = (messages: UdpMessage[]) =>
  Layer.effect(
    UdpSocketTag,
    Effect.gen(function* () {
      const queue = yield* Queue.unbounded<UdpMessage>();
      for (const msg of messages) {
        yield* Queue.offer(queue, msg);
      }
      return { queue };
    }),
  );
