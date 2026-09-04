export type PromptCatalogItem = {
  id: string;
  title: string;
  category: string;
  content: string;
  enabled: boolean;
  source: "codex-x" | "local";
  filename: string;
  description: string;
};

export const CODEX_X_PROMPT_CATALOG_URL =
  "https://api.github.com/repos/yynxxxxx/Codex-X/contents/examples?ref=main";
export const CODEX_X_PROMPT_RAW_BASE_URL =
  "https://raw.githubusercontent.com/yynxxxxx/Codex-X/main/examples/";

const CACHE_KEY = "codex-plus-codex-x-prompts";

type GithubEntry = {
  name?: string;
  type?: string;
  download_url?: string | null;
};

function titleFromFilename(filename: string): string {
  return filename
    .replace(/\.md$/i, "")
    .replace(/[-_]+/g, " ")
    .replace(/\b\w/g, (match) => match.toUpperCase());
}

function descriptionFromMarkdown(content: string, filename: string): string {
  const line = content
    .split(/\r?\n/)
    .map((value) => value.trim())
    .find((value) => value && !value.startsWith("#") && !value.startsWith("```"));
  return (line || `${titleFromFilename(filename)} prompt template`).slice(0, 140);
}

function categoryFromFilename(filename: string): string {
  const lower = filename.toLowerCase();
  if (lower.startsWith("writing-")) return "写作辅助";
  if (lower.startsWith("software-development-")) return "软件开发";
  return "破甲 / 逆向";
}

export function readPromptCatalogCache(): PromptCatalogItem[] {
  if (typeof localStorage === "undefined") return [];
  try {
    const value = JSON.parse(localStorage.getItem(CACHE_KEY) || "[]");
    return Array.isArray(value) ? value : [];
  } catch {
    return [];
  }
}

function writePromptCatalogCache(items: PromptCatalogItem[]): void {
  if (typeof localStorage === "undefined") return;
  localStorage.setItem(CACHE_KEY, JSON.stringify(items));
}

export async function fetchCodexXPromptCatalog(
  fetchImpl: typeof fetch = fetch,
): Promise<PromptCatalogItem[]> {
  const listingResponse = await fetchImpl(CODEX_X_PROMPT_CATALOG_URL, {
    headers: { Accept: "application/vnd.github+json" },
  });
  if (!listingResponse.ok) throw new Error(`GitHub template index returned ${listingResponse.status}`);
  const entries = (await listingResponse.json()) as GithubEntry[];
  const markdownEntries = entries.filter(
    (entry) => entry.type === "file" && typeof entry.name === "string" && /\.md$/i.test(entry.name),
  );
  const items = await Promise.all(
    markdownEntries.map(async (entry) => {
      const filename = entry.name as string;
      const url = entry.download_url || `${CODEX_X_PROMPT_RAW_BASE_URL}${encodeURIComponent(filename)}`;
      const response = await fetchImpl(url);
      if (!response.ok) throw new Error(`Unable to read ${filename}`);
      const content = await response.text();
      return {
        id: `codex-x:${filename.toLowerCase()}`,
        title: titleFromFilename(filename),
        category: categoryFromFilename(filename),
        content,
        enabled: false,
        source: "codex-x" as const,
        filename,
        description: descriptionFromMarkdown(content, filename),
      } satisfies PromptCatalogItem;
    }),
  );
  const deduped = Array.from(new Map(items.map((item) => [item.id, item])).values());
  writePromptCatalogCache(deduped);
  return deduped;
}

export async function syncCodexXPromptCatalog(
  fetchImpl: typeof fetch = fetch,
): Promise<{ items: PromptCatalogItem[]; fromCache: boolean }> {
  try {
    return { items: await fetchCodexXPromptCatalog(fetchImpl), fromCache: false };
  } catch {
    const cached = readPromptCatalogCache();
    if (cached.length) return { items: cached, fromCache: true };
    throw new Error("Codex-X 模板暂时无法读取，请检查网络连接。");
  }
}
