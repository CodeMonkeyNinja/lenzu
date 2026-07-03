import * as grpc from "@grpc/grpc-js";
import * as protoLoader from "@grpc/proto-loader";
import * as path from "path";
import { BrowserWindow, app } from "electron";
import { Effect } from "effect";
import {
  handleSendText,
  handleMoveWindow,
  handleShutdown,
} from "./grpc-handlers";
import type { HudConfig } from "../config";

export interface GrpcServerOptions {
  readonly port: number;
  readonly mainWindow: BrowserWindow;
  readonly config: HudConfig;
}

export function startGrpcServer(opts: GrpcServerOptions): Effect.Effect<void> {
  return Effect.sync(() => {
    const protoPath = path.join(__dirname, "proto", "lenzu_hud.proto");
    const packageDefinition = protoLoader.loadSync(protoPath, {
      keepCase: false,
      longs: String,
      enums: String,
      defaults: true,
      oneofs: true,
    });
    const proto = grpc.loadPackageDefinition(packageDefinition) as any;

    const server = new grpc.Server();

    server.addService(proto.lenzu_hud.LenzuHud.service, {
      sendText: (
        call: grpc.ServerUnaryCall<any, any>,
        callback: grpc.sendUnaryData<any>,
      ) => {
        handleSendText(call.request.text ?? "", liveHudLayer(opts.mainWindow))
          .then(() => callback(null, { ok: true }))
          .catch((e) => {
            console.error("[gRPC] sendText error:", e);
            callback(null, { ok: false });
          });
      },

      moveWindow: (
        call: grpc.ServerUnaryCall<any, any>,
        callback: grpc.sendUnaryData<any>,
      ) => {
        const pos = call.request.pos;
        if (pos !== "top" && pos !== "bottom") {
          callback(null, { ok: false });
          return;
        }
        handleMoveWindow(pos, opts.mainWindow, opts.config);
        callback(null, { ok: true });
      },

      shutdown: (
        _call: grpc.ServerUnaryCall<any, any>,
        callback: grpc.sendUnaryData<any>,
      ) => {
        callback(null, { ok: true });
        handleShutdown();
      },
    });

    const addr = `127.0.0.1:${opts.port}`;
    server.bindAsync(addr, grpc.ServerCredentials.createInsecure(), (err) => {
      if (err) {
        console.error(`[gRPC] failed to bind ${addr}:`, err);
        return;
      }
      server.start();
      console.log(`[gRPC] server listening on ${addr}`);
    });
  });
}
