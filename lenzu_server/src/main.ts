import { app, BrowserWindow, ipcMain, screen } from "electron";
import * as path from "path";
import { execSync } from "child_process";
import { Effect, Layer, Queue } from "effect";
import { HudConfigService, HudConfigLive } from "./services/hud-config";
import { HudWindowTag, HudWindowLive } from "./services/hud-window";
import { UdpSocketTag, UdpSocketLive } from "./services/udp-socket";
import type { HudConfig } from "./config";
import { computePosition, type WindowPosition } from "./window-position";
import { processMessage } from "./dispatch";

// Electron 36–41 had a Linux X11 regression where transparent:true rendered
// as opaque white.  Re-tested on 41.3.0 (2026-04-25) and confirmed fixed.
console.info(`[HUD] Electron ${process.versions.electron} starting.`);

app.commandLine.appendSwitch("enable-transparent-visuals");

app.on("window-all-closed", () => {
  app.quit();
});

// ─── pure helpers ───────────────────────────────────────────────────────────

function createMainWindow(config: HudConfig): BrowserWindow {
  const primaryDisplay = screen.getPrimaryDisplay();
  const screenWidth = primaryDisplay.workAreaSize.width;

  const win = new BrowserWindow({
    width: screenWidth,
    height: config.height,
    transparent: true,
    frame: false,
    show: false,
    alwaysOnTop: true,
    skipTaskbar: true,
    resizable: false,
    hasShadow: false,
    focusable: false,
    backgroundColor: "#00000000",
    webPreferences: {
      preload: path.join(__dirname, "preload.js"),
      contextIsolation: true,
      nodeIntegration: false,
    },
  });

  const buf = win.getNativeWindowHandle();
  const winIdNum =
    buf.length >= 8 ? Number(buf.readBigUInt64LE(0)) : buf.readUInt32LE(0);
  try {
    const helper = app.isPackaged
      ? path.join(process.resourcesPath, "hud-set-override-redirect")
      : path.join(__dirname, "hud-set-override-redirect");
    execSync(`"${helper}" ${winIdNum}`);
  } catch (e) {
    console.warn(
      "[HUD] override_redirect helper failed (non-fatal):",
      (e as Error).message,
    );
  }

  win.loadFile(path.join(__dirname, "renderer/index.html"));
  win.once("ready-to-show", () => win.show());
  win.on("closed", () => app.quit());

  return win;
}

function positionWindow(
  position: WindowPosition,
  win: BrowserWindow | null,
  cfg: HudConfig,
): void {
  if (!win || win.isDestroyed()) return;
  const cursor = screen.getCursorScreenPoint();
  const display = screen.getDisplayNearestPoint(cursor);
  const winSize = win.getSize() as [number, number];
  const { x, y } = computePosition(
    display.workArea,
    winSize,
    position,
    cfg.bottom_margin,
  );
  win.setPosition(x, y);
}

// ─── program ────────────────────────────────────────────────────────────────

const program = Effect.gen(function* () {
  const config = yield* HudConfigService;

  yield* Effect.promise(() => app.whenReady());

  const mainWindow = createMainWindow(config);
  positionWindow("bottom", mainWindow, config);

  ipcMain.handle("get-config", () => config);
  ipcMain.on("move-window", (_event, position: string) => {
    if (position === "top" || position === "center" || position === "bottom") {
      positionWindow(position, mainWindow, config);
    }
  });

  const { queue } = yield* UdpSocketTag;

  const handlePosition = (pos: "top" | "bottom") =>
    Effect.sync(() => positionWindow(pos, mainWindow, config));
  const handleShutdown = Effect.sync(() => app.quit());

  yield* Effect.forever(
    Effect.gen(function* () {
      const cmd = yield* Queue.take(queue);
      return yield* processMessage(cmd, handleShutdown, handlePosition);
    }).pipe(Effect.provide(HudWindowLive(mainWindow))),
  );
});

// ─── layers ─────────────────────────────────────────────────────────────────

const AppLayer = Layer.provideMerge(
  UdpSocketLive,
  HudConfigLive(app.getAppPath()),
);

// ─── entry point ────────────────────────────────────────────────────────────

Effect.runPromise(program.pipe(Effect.provide(AppLayer))).catch((e) => {
  console.error("[HUD] fatal error:", e);
  process.exit(1);
});
