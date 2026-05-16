import { readArticleBySlug } from "./articles.js";
import { docs } from "./docs.js";

let cached = null;

function stripMarkdown(md) {
  return md
    .replace(/```[\s\S]*?```/g, "")
    .replace(/`[^`]+`/g, "")
    .replace(/\[([^\]]+)\]\([^)]+\)/g, "$1")
    .replace(/^#{1,6}\s+/gm, "")
    .replace(/\*{1,3}([^*]+)\*{1,3}/g, "$1")
    .replace(/<[^>]+>/g, "")
    .replace(/\n{3,}/g, "\n\n")
    .trim();
}

export async function getSearchIndex() {
  if (cached) return cached;

  const entries = [];
  for (const doc of docs) {
    try {
      const raw = await readArticleBySlug(doc.slug);
      const content = raw ? stripMarkdown(raw) : "";
      entries.push({
        title: doc.title,
        href: doc.path,
        section: doc.section ?? "",
        content,
      });
    } catch {
      entries.push({
        title: doc.title,
        href: doc.path,
        section: doc.section ?? "",
        content: "",
      });
    }
  }

  cached = entries;
  return entries;
}
