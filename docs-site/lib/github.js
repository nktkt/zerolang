const REPO = process.env.NEXT_PUBLIC_GITHUB_REPO || "vercel-labs/zero";
const REVALIDATE = 86400;
const FETCH_STARS = process.env.ZERO_DOCS_FETCH_GITHUB_STARS === "1";

function formatStarCount(count) {
  if (typeof count !== "number") return "";
  if (count >= 1000) return `${(count / 1000).toFixed(count >= 10000 ? 0 : 1)}k`;
  return String(count);
}

export async function getStarCount() {
  const configured = Number(process.env.NEXT_PUBLIC_GITHUB_STARS);
  if (Number.isFinite(configured) && configured >= 0) return formatStarCount(configured);
  if (!FETCH_STARS) return "";

  try {
    const res = await fetch(`https://api.github.com/repos/${REPO}`, {
      headers: { Accept: "application/vnd.github.v3+json" },
      next: { revalidate: REVALIDATE },
      signal: AbortSignal.timeout(1500),
    });
    if (!res.ok) return "";
    const data = await res.json();
    return formatStarCount(data.stargazers_count);
  } catch {
    return "";
  }
}
