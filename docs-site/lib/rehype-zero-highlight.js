import { highlight } from "./highlight.js";

const ZERO_TOKEN_COLORS = {
  comment: { light: "#6A737D", dark: "#6A737D" },
  string: { light: "#032F62", dark: "#9ECBFF" },
  char: { light: "#032F62", dark: "#9ECBFF" },
  keyword: { light: "#D73A49", dark: "#F97583" },
  type: { light: "#005CC5", dark: "#79B8FF" },
  function: { light: "#6F42C1", dark: "#B392F0" },
  number: { light: "#005CC5", dark: "#79B8FF" },
  variable: { light: "#24292E", dark: "#E1E4E8" },
  punctuation: { light: "#24292E", dark: "#E1E4E8" },
  operator: { light: "#D73A49", dark: "#F97583" },
};

function visit(node, predicate, callback) {
  if (!node) return;
  if (predicate(node)) callback(node);
  if (Array.isArray(node.children)) {
    for (const child of node.children) visit(child, predicate, callback);
  }
}

function highlightZeroToHast(code) {
  const parts = highlight(code, "zero").split("\n");
  const lines = [];
  for (let li = 0; li < parts.length; li++) {
    const lineHtml = parts[li];
    const tokens = [];
    const tokenRegex = /<span class="hl-(\w+)">([\s\S]*?)<\/span>|([^<]+)/g;
    let match;
    while ((match = tokenRegex.exec(lineHtml)) !== null) {
      if (match[1]) {
        const color = ZERO_TOKEN_COLORS[match[1]];
        const text = decodeEntities(match[2]);
        tokens.push({
          type: "element",
          tagName: "span",
          properties: color
            ? { style: `--shiki-light:${color.light};--shiki-dark:${color.dark}` }
            : {},
          children: [{ type: "text", value: text }],
        });
      } else if (match[3]) {
        tokens.push({ type: "text", value: decodeEntities(match[3]) });
      }
    }
    lines.push({
      type: "element",
      tagName: "span",
      properties: { "data-line": "" },
      children: tokens.length > 0 ? tokens : [{ type: "text", value: "" }],
    });
  }
  return lines;
}

function decodeEntities(value) {
  return value
    .replaceAll("&amp;", "&")
    .replaceAll("&lt;", "<")
    .replaceAll("&gt;", ">")
    .replaceAll("&quot;", '"')
    .replaceAll("&#039;", "'");
}

function getNodeText(node) {
  if (!node) return "";
  if (node.type === "text") return node.value ?? "";
  if (!Array.isArray(node.children)) return "";
  return node.children.map(getNodeText).join("");
}

export function rehypeZeroHighlight() {
  return (tree) => {
    visit(
      tree,
      (node) =>
        node.type === "element" &&
        node.tagName === "code" &&
        (node.properties?.dataLanguage === "zero" ||
          node.properties?.["data-language"] === "zero"),
      (codeNode) => {
        const text = getNodeText(codeNode).replace(/\n$/, "");
        codeNode.children = highlightZeroToHast(text);
        codeNode.properties = {
          ...codeNode.properties,
          style: "display:grid",
        };
      },
    );
  };
}
