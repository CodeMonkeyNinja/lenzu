import * as fs from "fs";
import { Schema, Effect, Data } from "effect";

export interface HudConfig {
  height: number;
  background_opacity: number;
  text_color: string;
  font_size_pt: number;
  min_font_size_pt: number;
  bottom_margin: number;
  udp_port: number;
  default_text: string;
}

export const DEFAULT_CONFIG: HudConfig = {
  height: 200,
  background_opacity: 0.72,
  text_color: "#f5e642",
  font_size_pt: 24,
  min_font_size_pt: 0,
  bottom_margin: 50,
  udp_port: 7331,
  default_text: "Hello world, Hello Shiroe!",
};

const HudConfigSchema = Schema.partial(
  Schema.Struct({
    height: Schema.Number,
    background_opacity: Schema.Number.pipe(Schema.between(0, 1)),
    text_color: Schema.String,
    font_size_pt: Schema.Number,
    min_font_size_pt: Schema.Number,
    bottom_margin: Schema.Number,
    udp_port: Schema.Number.pipe(Schema.between(1, 65535)),
    default_text: Schema.String,
  }),
);

export class ConfigFileNotFound extends Data.TaggedError("ConfigFileNotFound")<{
  path: string;
  cause: unknown;
}> {}

export class ConfigParseError extends Data.TaggedError("ConfigParseError")<{
  cause: unknown;
}> {}

export const loadConfig = (
  configPath: string,
): Effect.Effect<HudConfig, ConfigFileNotFound | ConfigParseError> =>
  Effect.gen(function* () {
    const raw = yield* Effect.try({
      try: () => fs.readFileSync(configPath, "utf-8"),
      catch: (e) => new ConfigFileNotFound({ path: configPath, cause: e }),
    });
    const parsed = yield* Effect.try({
      try: () => JSON.parse(raw),
      catch: (e) => new ConfigParseError({ cause: e }),
    });
    const partial = yield* Schema.decodeUnknown(HudConfigSchema, {
      onExcessProperty: "ignore",
    })(parsed).pipe(Effect.mapError((e) => new ConfigParseError({ cause: e })));
    return { ...DEFAULT_CONFIG, ...partial };
  });

export function loadConfigSync(configPath: string): HudConfig {
  return Effect.runSync(
    loadConfig(configPath).pipe(
      Effect.catchTag("ConfigFileNotFound", () =>
        Effect.succeed(DEFAULT_CONFIG),
      ),
      Effect.catchTag("ConfigParseError", (e) =>
        Effect.succeed(DEFAULT_CONFIG).pipe(
          Effect.tap(
            Effect.logWarning(
              `hud_config.json parse error — using defaults: ${e.cause}`,
            ),
          ),
        ),
      ),
    ),
  );
}
