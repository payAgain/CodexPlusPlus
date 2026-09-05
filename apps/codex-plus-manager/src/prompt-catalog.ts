export type PromptRepository = {
  id: string;
  name: string;
  owner: string;
  repository: string;
  branch: string;
  directory: string;
  enabled: boolean;
  builtIn?: boolean;
};

export type PromptCatalogItem = {
  id: string;
  title: string;
  category: string;
  content: string;
  enabled: boolean;
  source: "codex-x" | "repository" | "local";
  repositoryId?: string;
  repositoryName?: string;
  filename: string;
  description: string;
};

export const DEFAULT_PROMPT_REPOSITORY: PromptRepository = {
  id: "github:yynxxxxx/codex-x:main:examples",
  name: "Codex-X",
  owner: "yynxxxxx",
  repository: "Codex-X",
  branch: "main",
  directory: "examples",
  enabled: true,
  builtIn: true,
};

const CACHE_KEY = "codex-plus-prompt-catalog";
const LEGACY_CACHE_KEY = "codex-plus-codex-x-prompts";
const REPOSITORIES_KEY = "codex-plus-prompt-repositories";

type GithubEntry = { name?: string; path?: string; type?: string; download_url?: string | null };

function storage(): Storage | null { return typeof localStorage === "undefined" ? null : localStorage; }
function normalizedDirectory(value: string): string { return value.trim().replace(/^\/+|\/+$/g, ""); }

export function promptRepositoryId(repository: Pick<PromptRepository, "owner" | "repository" | "branch" | "directory">): string {
  return `github:${repository.owner.toLowerCase()}/${repository.repository.toLowerCase()}:${repository.branch}:${normalizedDirectory(repository.directory)}`;
}

export function createPromptRepository(input: { name: string; url: string; branch: string; directory: string }): PromptRepository {
  const url = new URL(input.url.trim());
  if (url.protocol !== "https:" || !["github.com", "www.github.com"].includes(url.hostname)) throw new Error("请输入有效的 GitHub 仓库地址。");
  const parts = url.pathname.replace(/^\/+|\/+$/g, "").split("/");
  if (parts.length !== 2 || !parts[0] || !parts[1]) throw new Error("GitHub 地址必须是仓库主页地址。");
  const value = { name: input.name.trim() || parts[1].replace(/\.git$/i, ""), owner: parts[0], repository: parts[1].replace(/\.git$/i, ""), branch: input.branch.trim() || "main", directory: normalizedDirectory(input.directory), enabled: true };
  return { ...value, id: promptRepositoryId(value) };
}

export function readPromptRepositories(): PromptRepository[] {
  const store = storage();
  if (!store) return [DEFAULT_PROMPT_REPOSITORY];
  try {
    const value = JSON.parse(store.getItem(REPOSITORIES_KEY) || "[]") as PromptRepository[];
    if (!Array.isArray(value) || !value.length) return [DEFAULT_PROMPT_REPOSITORY];
    return value.map((item) => item.id === DEFAULT_PROMPT_REPOSITORY.id ? { ...item, builtIn: true } : item);
  } catch { return [DEFAULT_PROMPT_REPOSITORY]; }
}

export function writePromptRepositories(repositories: PromptRepository[]): void { storage()?.setItem(REPOSITORIES_KEY, JSON.stringify(repositories)); }

function titleFromFilename(filename: string): string { return filename.replace(/\.md$/i, "").replace(/[-_]+/g, " ").replace(/\b\w/g, (match) => match.toUpperCase()); }
function descriptionFromMarkdown(content: string, filename: string): string {
  const line = content.split(/\r?\n/).map((value) => value.trim()).find((value) => value && !value.startsWith("#") && !value.startsWith("```"));
  return (line || `${titleFromFilename(filename)} prompt template`).slice(0, 140);
}
function categoryFromFilename(filename: string): string {
  const lower = filename.toLowerCase();
  if (lower.startsWith("writing-")) return "写作辅助";
  if (lower.startsWith("software-development-")) return "软件开发";
  return "破甲 / 逆向";
}

export function readPromptCatalogCache(): PromptCatalogItem[] {
  const store = storage();
  if (!store) return [];
  try { const value = JSON.parse(store.getItem(CACHE_KEY) || store.getItem(LEGACY_CACHE_KEY) || "[]"); return Array.isArray(value) ? value : []; } catch { return []; }
}
function writePromptCatalogCache(items: PromptCatalogItem[]): void { storage()?.setItem(CACHE_KEY, JSON.stringify(items)); }

export async function fetchPromptRepositoryCatalog(repository: PromptRepository, fetchImpl: typeof fetch = fetch): Promise<PromptCatalogItem[]> {
  const directory = normalizedDirectory(repository.directory);
  const contentsPath = directory ? `/contents/${directory.split("/").map(encodeURIComponent).join("/")}` : "/contents";
  const listingUrl = `https://api.github.com/repos/${encodeURIComponent(repository.owner)}/${encodeURIComponent(repository.repository)}${contentsPath}?ref=${encodeURIComponent(repository.branch)}`;
  const listingResponse = await fetchImpl(listingUrl, { headers: { Accept: "application/vnd.github+json" } });
  if (!listingResponse.ok) throw new Error(`${repository.name}: GitHub template index returned ${listingResponse.status}`);
  const entries = (await listingResponse.json()) as GithubEntry[];
  const markdownEntries = entries.filter((entry) => entry.type === "file" && typeof entry.name === "string" && /\.md$/i.test(entry.name));
  return Promise.all(markdownEntries.map(async (entry) => {
    const filename = entry.name as string;
    const fallbackPath = entry.path || [directory, filename].filter(Boolean).join("/");
    const url = entry.download_url || `https://raw.githubusercontent.com/${repository.owner}/${repository.repository}/${repository.branch}/${fallbackPath.split("/").map(encodeURIComponent).join("/")}`;
    const response = await fetchImpl(url);
    if (!response.ok) throw new Error(`${repository.name}: Unable to read ${filename}`);
    const content = await response.text();
    return { id: `${repository.id}:${fallbackPath.toLowerCase()}`, title: titleFromFilename(filename), category: categoryFromFilename(filename), content, enabled: false, source: repository.id === DEFAULT_PROMPT_REPOSITORY.id ? "codex-x" as const : "repository" as const, repositoryId: repository.id, repositoryName: repository.name, filename, description: descriptionFromMarkdown(content, filename) } satisfies PromptCatalogItem;
  }));
}

export async function fetchCodexXPromptCatalog(fetchImpl: typeof fetch = fetch): Promise<PromptCatalogItem[]> { return fetchPromptRepositoryCatalog(DEFAULT_PROMPT_REPOSITORY, fetchImpl); }

export async function syncPromptRepositories(repositories: PromptRepository[], fetchImpl: typeof fetch = fetch): Promise<{ items: PromptCatalogItem[]; fromCache: boolean; errors: string[] }> {
  const enabled = repositories.filter((repository) => repository.enabled);
  const results = await Promise.allSettled(enabled.map((repository) => fetchPromptRepositoryCatalog(repository, fetchImpl)));
  const items: PromptCatalogItem[] = [];
  const errors: string[] = [];
  results.forEach((result, index) => { if (result.status === "fulfilled") items.push(...result.value); else errors.push(result.reason instanceof Error ? result.reason.message : `${enabled[index].name}: sync failed`); });
  if (items.length) {
    const deduped = Array.from(new Map(items.map((item) => [item.id, item])).values());
    writePromptCatalogCache(deduped);
    return { items: deduped, fromCache: false, errors };
  }
  const cached = readPromptCatalogCache().filter((item) => !item.repositoryId || enabled.some((repo) => repo.id === item.repositoryId));
  if (cached.length) return { items: cached, fromCache: true, errors };
  throw new Error(errors.join("；") || "提示词仓库暂时无法读取，请检查网络连接。");
}

export async function syncCodexXPromptCatalog(fetchImpl: typeof fetch = fetch): Promise<{ items: PromptCatalogItem[]; fromCache: boolean }> {
  const result = await syncPromptRepositories([DEFAULT_PROMPT_REPOSITORY], fetchImpl);
  return { items: result.items, fromCache: result.fromCache };
}
