import test from "node:test";
import assert from "node:assert/strict";
import { createPromptRepository, fetchCodexXPromptCatalog, fetchPromptRepositoryCatalog } from "./prompt-catalog.ts";

test("fetchCodexXPromptCatalog reads markdown examples and classifies them", async () => {
  const fetchImpl = (async (input: RequestInfo | URL) => {
    const url = String(input);
    if (url.includes("api.github.com")) {
      return new Response(JSON.stringify([
        { name: "software-development-debugging.md", type: "file", download_url: "https://example.test/debug.md" },
        { name: "README.txt", type: "file", download_url: "https://example.test/readme.txt" },
      ]), { status: 200 });
    }
    return new Response(`# Debug

Inspect the failing path before changing code.`, { status: 200 });
  }) as typeof fetch;

  const items = await fetchCodexXPromptCatalog(fetchImpl);
  assert.equal(items.length, 1);
  assert.equal(items[0].category, "软件开发");
  assert.equal(items[0].title, "Software Development Debugging");
  assert.match(items[0].description, /Inspect the failing path/);
});

test("custom prompt repositories use the configured branch and directory", async () => {
  const repository = createPromptRepository({
    name: "Team prompts",
    url: "https://github.com/acme/prompts",
    branch: "develop",
    directory: "library",
  });
  const requested: string[] = [];
  const fetchImpl = (async (input: RequestInfo | URL) => {
    const url = String(input);
    requested.push(url);
    if (url.includes("api.github.com")) {
      return new Response(JSON.stringify([{ name: "review.md", path: "library/review.md", type: "file", download_url: null }]), { status: 200 });
    }
    return new Response("Review code carefully.", { status: 200 });
  }) as typeof fetch;

  const items = await fetchPromptRepositoryCatalog(repository, fetchImpl);
  assert.equal(items[0].source, "repository");
  assert.equal(items[0].repositoryName, "Team prompts");
  assert.match(requested[0], /develop/);
  assert.match(requested[0], /library/);
  assert.match(requested[1], /raw\.githubusercontent\.com\/acme\/prompts\/develop\/library\/review\.md/);
});
