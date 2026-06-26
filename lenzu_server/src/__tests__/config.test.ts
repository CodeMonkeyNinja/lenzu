import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import * as fs from "fs";
import * as path from "path";
import * as os from "os";
import { Effect } from "effect";
import {
  loadConfig,
  loadConfigSync,
  DEFAULT_CONFIG,
  ConfigFileNotFound,
  ConfigParseError,
} from "../config";

describe("loadConfig", () => {
  let tmpDir: string;

  beforeEach(() => {
    tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "hud-test-"));
  });

  afterEach(() => {
    fs.rmSync(tmpDir, { recursive: true });
  });

  it("returns defaults when file does not exist", () => {
    const result = Effect.runSync(
      loadConfig(path.join(tmpDir, "missing.json")).pipe(
        Effect.catchTag("ConfigFileNotFound", () =>
          Effect.succeed(DEFAULT_CONFIG),
        ),
        Effect.catchTag("ConfigParseError", () =>
          Effect.succeed(DEFAULT_CONFIG),
        ),
      ),
    );
    expect(result).toEqual(DEFAULT_CONFIG);
  });

  it("returns ConfigFileNotFound error when file does not exist", () => {
    const result = Effect.runSync(
      loadConfig(path.join(tmpDir, "missing.json")).pipe(
        Effect.catchTag("ConfigParseError", () =>
          Effect.succeed(DEFAULT_CONFIG),
        ),
        Effect.catchAll((e) => Effect.succeed(e)),
      ),
    );
    expect(result).toBeInstanceOf(ConfigFileNotFound);
    expect((result as ConfigFileNotFound).path).toBe(
      path.join(tmpDir, "missing.json"),
    );
  });

  it("merges file values over defaults", () => {
    const configPath = path.join(tmpDir, "hud_config.json");
    fs.writeFileSync(
      configPath,
      JSON.stringify({ udp_port: 9999, text_color: "#ff0000" }),
    );
    const cfg = Effect.runSync(
      loadConfig(configPath).pipe(
        Effect.catchTag("ConfigFileNotFound", () =>
          Effect.succeed(DEFAULT_CONFIG),
        ),
        Effect.catchTag("ConfigParseError", () =>
          Effect.succeed(DEFAULT_CONFIG),
        ),
      ),
    );
    expect(cfg.udp_port).toBe(9999);
    expect(cfg.text_color).toBe("#ff0000");
    expect(cfg.height).toBe(DEFAULT_CONFIG.height);
    expect(cfg.font_size_pt).toBe(DEFAULT_CONFIG.font_size_pt);
  });

  it("returns ConfigParseError when JSON is malformed", () => {
    const configPath = path.join(tmpDir, "hud_config.json");
    fs.writeFileSync(configPath, "not valid json {{{");
    const result = Effect.runSync(
      loadConfig(configPath).pipe(
        Effect.catchTag("ConfigFileNotFound", () =>
          Effect.succeed(DEFAULT_CONFIG),
        ),
        Effect.catchAll((e) => Effect.succeed(e)),
      ),
    );
    expect(result).toBeInstanceOf(ConfigParseError);
  });

  it("returns defaults when file is empty", () => {
    const configPath = path.join(tmpDir, "hud_config.json");
    fs.writeFileSync(configPath, "");
    const result = Effect.runSync(
      loadConfig(configPath).pipe(
        Effect.catchTag("ConfigFileNotFound", () =>
          Effect.succeed(DEFAULT_CONFIG),
        ),
        Effect.catchAll((e) => Effect.succeed(e)),
      ),
    );
    expect(result).toBeInstanceOf(ConfigParseError);
  });

  it("does not mutate DEFAULT_CONFIG", () => {
    const before = { ...DEFAULT_CONFIG };
    const configPath = path.join(tmpDir, "hud_config.json");
    fs.writeFileSync(configPath, JSON.stringify({ udp_port: 1234 }));
    Effect.runSync(
      loadConfig(configPath).pipe(
        Effect.catchTag("ConfigFileNotFound", () =>
          Effect.succeed(DEFAULT_CONFIG),
        ),
        Effect.catchTag("ConfigParseError", () =>
          Effect.succeed(DEFAULT_CONFIG),
        ),
      ),
    );
    expect(DEFAULT_CONFIG).toEqual(before);
  });
});

describe("loadConfigSync", () => {
  let tmpDir: string;

  beforeEach(() => {
    tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "hud-test-"));
  });

  afterEach(() => {
    fs.rmSync(tmpDir, { recursive: true });
  });

  it("returns defaults when file does not exist", () => {
    const cfg = loadConfigSync(path.join(tmpDir, "missing.json"));
    expect(cfg).toEqual(DEFAULT_CONFIG);
  });

  it("merges file values over defaults", () => {
    const configPath = path.join(tmpDir, "hud_config.json");
    fs.writeFileSync(
      configPath,
      JSON.stringify({ udp_port: 9999, text_color: "#ff0000" }),
    );
    const cfg = loadConfigSync(configPath);
    expect(cfg.udp_port).toBe(9999);
    expect(cfg.text_color).toBe("#ff0000");
    expect(cfg.height).toBe(DEFAULT_CONFIG.height);
    expect(cfg.font_size_pt).toBe(DEFAULT_CONFIG.font_size_pt);
  });

  it("returns defaults when JSON is malformed", () => {
    vi.spyOn(console, "warn").mockImplementation(() => {});
    const configPath = path.join(tmpDir, "hud_config.json");
    fs.writeFileSync(configPath, "not valid json {{{");
    const cfg = loadConfigSync(configPath);
    vi.restoreAllMocks();
    expect(cfg).toEqual(DEFAULT_CONFIG);
  });

  it("does not mutate DEFAULT_CONFIG", () => {
    const before = { ...DEFAULT_CONFIG };
    const configPath = path.join(tmpDir, "hud_config.json");
    fs.writeFileSync(configPath, JSON.stringify({ udp_port: 1234 }));
    loadConfigSync(configPath);
    expect(DEFAULT_CONFIG).toEqual(before);
  });
});
