import { describe, it, expect } from "vitest";
import { sanitizeHtml } from "../renderer/sanitize";

describe("sanitizeHtml", () => {
  // ── Happy paths ──────────────────────────────────────────────────────────

  it("allows ruby element with rt annotation", () => {
    expect(sanitizeHtml("<ruby>食<rt>た</rt></ruby>")).toBe(
      "<ruby>食<rt>た</rt></ruby>",
    );
  });

  it("allows multiple ruby annotations inline", () => {
    const input = "<ruby>食<rt>た</rt></ruby>べ<ruby>物<rt>もの</rt></ruby>";
    expect(sanitizeHtml(input)).toBe(input);
  });

  it("strips attributes from ruby tags", () => {
    expect(sanitizeHtml('<ruby class="evil">食<rt>た</rt></ruby>')).toBe(
      "<ruby>食<rt>た</rt></ruby>",
    );
  });

  it("allows furigana span with read and base children", () => {
    const input =
      '<span class="furigana"><span class="read">た</span><span class="base">食</span></span>';
    expect(sanitizeHtml(input)).toBe(input);
  });

  it("passes through plain text unchanged", () => {
    expect(sanitizeHtml("hello world")).toBe("hello world");
  });

  it("passes through Japanese text unchanged", () => {
    expect(sanitizeHtml("日本語")).toBe("日本語");
  });

  // ── Security: strip dangerous tags ───────────────────────────────────────

  it("strips script tags", () => {
    expect(sanitizeHtml("<script>alert(1)</script>hello")).toBe("hello");
  });

  it("strips img tags", () => {
    expect(sanitizeHtml("<img src=x onerror=alert(1)>hello")).toBe("hello");
  });

  it("strips iframe tags", () => {
    expect(sanitizeHtml('<iframe src="https://evil"></iframe>hello')).toBe(
      "hello",
    );
  });

  it("strips style tags", () => {
    expect(sanitizeHtml("<style>body{color:red}</style>hello")).toBe("hello");
  });

  it("strips anchor tags", () => {
    expect(sanitizeHtml('<a href="https://evil">click</a>hello')).toBe(
      "clickhello",
    );
  });

  // ── Security: strip dangerous attributes ─────────────────────────────────

  it("strips onclick attribute from span", () => {
    expect(sanitizeHtml('<span onclick="alert(1)">hello</span>')).toBe(
      "<span>hello</span>",
    );
  });

  it("strips onerror attribute from span", () => {
    expect(sanitizeHtml('<span onerror="alert(1)">hello</span>')).toBe(
      "<span>hello</span>",
    );
  });

  it("strips style attribute from span", () => {
    expect(sanitizeHtml('<span style="color:red">hello</span>')).toBe(
      "<span>hello</span>",
    );
  });

  it("strips disallowed class values from span", () => {
    expect(sanitizeHtml('<span class="evil">hello</span>')).toBe(
      "<span>hello</span>",
    );
  });

  it("removes class attribute entirely when class is not furigana/read/base", () => {
    expect(sanitizeHtml('<span class="furigana evil">hello</span>')).toBe(
      "<span>hello</span>",
    );
  });

  // ── Edge cases ───────────────────────────────────────────────────────────

  it("passes through empty string", () => {
    expect(sanitizeHtml("")).toBe("");
  });

  it("escapes bare angle brackets in text", () => {
    // Bare < and > that aren't part of a valid tag should be escaped
    expect(sanitizeHtml("a < b > c")).toBe("a &lt; b &gt; c");
  });

  it("handles nested spans correctly", () => {
    const input =
      '<span class="furigana"><span class="read">とうきょう</span><span class="base">東京</span></span>';
    expect(sanitizeHtml(input)).toBe(input);
  });

  it("strips unknown tags but keeps their text content", () => {
    expect(sanitizeHtml("<b>bold</b> <i>italic</i>")).toBe("bold italic");
  });

  it("handles mixed safe and unsafe markup", () => {
    const input =
      '<span class="furigana"><span class="read">た</span></span>' +
      "<script>evil()</script>" +
      '<span class="base">食</span>';
    const expected =
      '<span class="furigana"><span class="read">た</span></span>' +
      '<span class="base">食</span>';
    expect(sanitizeHtml(input)).toBe(expected);
  });

  it("handles null bytes gracefully", () => {
    expect(sanitizeHtml("hello\0world")).toBe("helloworld");
  });
});
