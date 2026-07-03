const ALLOWED_CLASSES = new Set(["furigana", "read", "base"]);
const RAW_CONTENT_TAGS = new Set(["script", "style", "iframe"]);
const ALLOWED_TAGS = new Set(["ruby", "rt", "span"]);

function classListAllowed(classes: string): boolean {
  if (!classes.trim()) return false;
  return classes.split(/\s+/).every((c) => ALLOWED_CLASSES.has(c));
}

function escapeText(s: string): string {
  let out = "";
  for (const ch of s) {
    switch (ch) {
      case "<":
        out += "&lt;";
        break;
      case ">":
        out += "&gt;";
        break;
      case "\0":
        break;
      default:
        out += ch;
    }
  }
  return out;
}

function normalizeTagName(name: string): string {
  return name.toLowerCase();
}

function isWhitespace(c: string): boolean {
  return c === " " || c === "\t" || c === "\n" || c === "\r";
}

function isAlpha(c: string): boolean {
  return (c >= "a" && c <= "z") || (c >= "A" && c <= "Z");
}

function isTagChar(c: string): boolean {
  return isAlpha(c) || c === "-" || c === "_" || (c >= "0" && c <= "9");
}

export function sanitizeHtml(input: string): string {
  const out: string[] = [];
  const len = input.length;

  let i = 0;
  let textBuf = "";

  function flushText() {
    if (textBuf) {
      out.push(escapeText(textBuf));
      textBuf = "";
    }
  }

  while (i < len) {
    const c = input[i];

    if (c === "<" && i + 1 < len) {
      const next = input[i + 1];

      // Only parse as tag if followed by alpha, /, or !
      if (isAlpha(next) || next === "/" || next === "!") {
        i++;
        const tagStart = i;

        // Comment: <!-- ... -->
        if (next === "!" && input.startsWith("!--", i)) {
          const end = input.indexOf("-->", i + 3);
          i = end !== -1 ? end + 3 : len;
          continue;
        }

        // Closing tag: </tagname>
        if (next === "/") {
          i++;
          const closeStart = i;
          while (i < len && input[i] !== ">" && !isWhitespace(input[i])) i++;
          const closeTagName = normalizeTagName(input.slice(closeStart, i));
          while (i < len && input[i] !== ">") i++;
          if (i < len) i++;
          if (ALLOWED_TAGS.has(closeTagName)) {
            flushText();
            out.push(`</${closeTagName}>`);
          }
          continue;
        }

        // Opening tag: <tagname ...>
        const nameStart = i;
        while (i < len && isTagChar(input[i])) i++;
        const tagName = normalizeTagName(input.slice(nameStart, i));

        if (RAW_CONTENT_TAGS.has(tagName)) {
          const closeTag = `</${tagName}>`;
          const end = input.toLowerCase().indexOf(closeTag, i);
          i = end !== -1 ? end + closeTag.length : len;
          continue;
        }

        if (ALLOWED_TAGS.has(tagName)) {
          let allowedClass = false;
          let classValue = "";

          while (i < len && input[i] !== ">") {
            while (i < len && isWhitespace(input[i])) i++;
            if (i >= len || input[i] === ">") break;

            if (input[i] === "/") {
              i++;
              if (i < len && input[i] === ">") i++;
              break;
            }

            const attrStart = i;
            while (
              i < len &&
              input[i] !== "=" &&
              !isWhitespace(input[i]) &&
              input[i] !== ">"
            )
              i++;
            const attrName = input.slice(attrStart, i).toLowerCase();

            let attrValue = "";
            if (i < len && input[i] === "=") {
              i++;
              if (i < len && (input[i] === '"' || input[i] === "'")) {
                const quote = input[i];
                i++;
                const valStart = i;
                while (i < len && input[i] !== quote) i++;
                attrValue = input.slice(valStart, i);
                if (i < len) i++;
              } else {
                const valStart = i;
                while (i < len && !isWhitespace(input[i]) && input[i] !== ">")
                  i++;
                attrValue = input.slice(valStart, i);
              }
            }

            if (attrName === "class" && classListAllowed(attrValue)) {
              allowedClass = true;
              classValue = attrValue;
            }
            // Event handler attrs are silently dropped
          }

          if (i < len && input[i] === ">") i++;

          flushText();
          if (tagName === "span") {
            out.push(allowedClass ? `<span class="${classValue}">` : `<span>`);
          } else {
            out.push(`<${tagName}>`);
          }
        } else {
          // Disallowed tag: skip to >, text content goes through normal flow
          while (i < len && input[i] !== ">") i++;
          if (i < len) i++;
        }
      } else {
        // Not a valid tag start — treat as plain text
        textBuf += "<";
        i++;
      }
    } else {
      textBuf += c;
      i++;
    }
  }

  flushText();
  return out.join("");
}
