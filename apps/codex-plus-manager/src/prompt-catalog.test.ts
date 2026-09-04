import test from "node:test";
import assert from "node:assert/strict";
import { fetchCodexXPromptCatalog } from "./prompt-catalog.ts";

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
