/*
 * Add "Open in Playground" links to mq code blocks
 * Uses lz-string for URL compression (same as mq-playground)
 */
(function () {
  const PLAYGROUND_URL = "https://mqlang.org/playground";

  const DEFAULT_MARKDOWN = `# Sample Document

## Introduction

This is a sample markdown document for testing mq queries.

## Features

- Easy to use
- Powerful filtering
- Markdown transformation

## Code Example

\`\`\`js
console.log("Hello, mq!");
\`\`\`

## Links

[mq Documentation](https://mqlang.org/book)
`;

  // Load lz-string from CDN
  function loadLZString() {
    return new Promise((resolve, reject) => {
      if (typeof LZString !== "undefined") {
        resolve(LZString);
        return;
      }

      const script = document.createElement("script");
      script.src =
        "https://cdn.jsdelivr.net/npm/lz-string@1.5.0/libs/lz-string.min.js";
      script.onload = () => resolve(LZString);
      script.onerror = () => reject(new Error("Failed to load lz-string"));
      document.head.appendChild(script);
    });
  }

  // Generate playground share URL
  function generatePlaygroundUrl(code) {
    const shareData = {
      code,
      markdown: DEFAULT_MARKDOWN,
      options: {
        isUpdate: false,
        inputFormat: "markdown",
        listStyle: null,
        linkUrlStyle: null,
        linkTitleStyle: null,
      },
    };

    const compressed = LZString.compressToEncodedURIComponent(
      JSON.stringify(shareData),
    );
    return `${PLAYGROUND_URL}#${compressed}`;
  }

  // Create playground link element
  function createPlaygroundLink(code) {
    const link = document.createElement("a");
    link.className = "playground-link";
    link.textContent = "Open in Playground";
    link.title = "Open this code in mq Playground";
    link.href = "#";
    link.onclick = async (e) => {
      e.preventDefault();
      try {
        await loadLZString();
        const url = generatePlaygroundUrl(code);
        window.open(url, "_blank");
      } catch (error) {
        console.error("Failed to open playground:", error);
        alert("Failed to open playground. Please try again.");
      }
    };
    return link;
  }

  // Add playground links to all mq code blocks
  function addPlaygroundLinks() {
    const mqCodeBlocks = document.querySelectorAll("pre code.language-mq");

    mqCodeBlocks.forEach((codeBlock) => {
      const pre = codeBlock.parentElement;
      if (!pre || pre.querySelector(".playground-link")) {
        return; // Skip if already has a link
      }

      const code = codeBlock.textContent || "";
      // Skip empty code blocks or comment-only blocks
      if (!code.trim() || code.trim().startsWith("#") && !code.includes("\n")) {
        return;
      }

      // Create wrapper for the link
      const linkWrapper = document.createElement("div");
      linkWrapper.className = "playground-link-wrapper";
      linkWrapper.appendChild(createPlaygroundLink(code));

      // Insert after the pre element
      pre.parentElement.insertBefore(linkWrapper, pre.nextSibling);
    });
  }

  // Run after DOM is ready and highlight.js has processed
  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", () => {
      // Delay slightly to ensure highlight.js has processed
      setTimeout(addPlaygroundLinks, 100);
    });
  } else {
    setTimeout(addPlaygroundLinks, 100);
  }
})();
