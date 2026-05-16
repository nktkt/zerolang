import { readFile } from "node:fs/promises";
import { join } from "node:path";
import { docs, findDocBySlug } from "./docs.js";

const ARTICLES_ROOT = join(process.cwd(), "articles");

export async function readArticleBySlug(slug) {
  const doc = findDocBySlug(slug);
  if (!doc) return null;
  const relative = doc.sourcePath.replace(/^\/articles\//, "");
  const filePath = join(ARTICLES_ROOT, relative);
  return readFile(filePath, "utf8");
}

export async function readArticleByPath(routePath) {
  const doc = docs.find((d) => d.path === routePath);
  if (!doc) return null;
  const source = await readArticleBySlug(doc.slug);
  return { doc, source };
}

export function extractHeadings(markdown) {
  const lines = markdown.replace(/\r\n/g, "\n").split("\n");
  const headings = [];
  let inCodeBlock = false;

  for (const line of lines) {
    if (line.trim().startsWith("```")) {
      inCodeBlock = !inCodeBlock;
      continue;
    }
    if (inCodeBlock) continue;

    const match = /^(#{2,4})\s+(.+)$/.exec(line.trim());
    if (match) {
      const text = match[2].replace(/`([^`]+)`/g, "$1");
      headings.push({
        level: match[1].length,
        text,
        id: slugify(match[2]),
      });
    }
  }
  return headings;
}

function slugify(value) {
  return value
    .toLowerCase()
    .replace(/`([^`]+)`/g, "$1")
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
}
