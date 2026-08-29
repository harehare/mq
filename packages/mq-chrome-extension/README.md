# mq Chrome Extension

A Manifest V3 Chrome extension that lets you convert the page you're
looking at to Markdown, run an [mq](https://github.com/harehare/mq) query
against it, and copy the result.

It's built on [`mq-web`](../mq-web) (the WebAssembly build of mq), so all
conversion and filtering happens locally in the extension's popup —
nothing is sent to a server.

## What it does

1. **Extract page** — when you open the popup, it grabs the current tab's raw
   HTML and converts it to Markdown in the source pane. You can also edit or
   paste your own content into it.
2. **Run a query** — filters the source Markdown and shows the result.
   Queries run automatically shortly after you stop typing, or immediately
   with **Cmd/Ctrl+Enter** or the Run button.
3. **Reuse the result** — copy it or download it as `mq-result.md`; the
   result pane also shows the output size and execution time.

## Install (unpacked, not yet on the Chrome Web Store)

```sh
cd packages/mq-chrome-extension
pnpm install
pnpm build
```

Then in Chrome: go to `chrome://extensions`, enable **Developer mode**,
click **Load unpacked**, and select `packages/mq-chrome-extension/.output/chrome-mv3`.

Click the mq icon in the toolbar to open the popup.

## Known v1 limitations

- Extraction only reads the top frame; same-origin iframes on the page
  aren't captured.
- No live re-sync for single-page apps — click "Extract page" again after
  the page content changes.
- Chrome Web Store publishing isn't automated; `pnpm build` (or `pnpm run
  zip`) produces a local build you load unpacked or upload yourself.

## Development

```sh
pnpm install
pnpm dev     # wxt dev server with HMR
pnpm test    # vitest
pnpm lint    # oxlint
```
