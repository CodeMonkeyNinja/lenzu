import { Context, Layer, Effect } from "effect";
import * as path from "path";
import { HudConfig, DEFAULT_CONFIG, loadConfig } from "../config.js";

export class HudConfigService extends Context.Tag("HudConfigService")<
  HudConfigService,
  HudConfig
>() {}

export const HudConfigLive = (appPath: string) =>
  Layer.effect(
    HudConfigService,
    loadConfig(path.join(appPath, "hud_config.json")).pipe(
      Effect.catchTag("ConfigFileNotFound", () =>
        Effect.succeed(DEFAULT_CONFIG),
      ),
      Effect.catchTag("ConfigParseError", (e) =>
        Effect.succeed(DEFAULT_CONFIG).pipe(
          Effect.tap(
            Effect.logWarning(
              `config parse error — using defaults: ${e.cause}`,
            ),
          ),
        ),
      ),
    ),
  );

export const HudConfigTest = (cfg: Partial<HudConfig> = {}) =>
  Layer.succeed(HudConfigService, { ...DEFAULT_CONFIG, ...cfg });
