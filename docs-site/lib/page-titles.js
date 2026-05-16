import { docs } from "./docs.js";

const DOCS_TITLES = Object.fromEntries(
  docs.map((doc) => [doc.path.replace(/^\//, ""), doc.title]),
);

export const PAGE_TITLES = {
  "": "The programming language\nfor agents.",
  ...DOCS_TITLES,
};

export function getPageTitle(slug) {
  return slug in PAGE_TITLES ? PAGE_TITLES[slug] : null;
}
