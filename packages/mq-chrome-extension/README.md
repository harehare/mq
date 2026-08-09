# mq Chrome Extension

A Manifest V3 Chrome extension that lets you convert the page you're
looking at to Markdown, run an [mq](https://github.com/harehare/mq) query
against it, copy the result, and preview the filtered content directly on
the page.

It's built on [`mq-web`](../mq-web) (the WebAssembly build of mq), so all
conversion and filtering happens locally in the extension's side panel —
nothing is sent to a server.

## What it does

1. **Extract page** — grabs the current tab's raw HTML and converts it to
   Markdown. Both are kept, and you can switch between a Markdown tab and
   an HTML tab in the source pane, editing or pasting your own content in
   either.
2. **Run a query** — filters the active tab's source (Markdown or raw
   HTML — HTML is queried directly via mq's `html` input format, using
   the same selectors) and shows the result, with a Copy button.
3. **Preview on page** — converts the (filtered or unfiltered) Markdown
   back to HTML and toggles a full-page overlay showing it, so you can
   switch between the original page and the converted view without
   leaving the tab.

## Install (unpacked, not yet on the Chrome Web Store)

```sh
cd packages/mq-chrome-extension
pnpm install
pnpm build
```

Then in Chrome: go to `chrome://extensions`, enable **Developer mode**,
click **Load unpacked**, and select `packages/mq-chrome-extension/.output/chrome-mv3`.

Click the mq icon in the toolbar to open the side panel.

## Known v1 limitations

- Chrome's `activeTab` permission is granted only for the tab active when
  you last clicked the toolbar icon. If you switch tabs while the side
  panel stays open, clicking "Extract page" may fail — click the toolbar
  icon again on the new tab to re-grant access (the background script
  opens the panel itself on each click, specifically so this re-grant is
  reliable even when the panel is already open).
- Pages with a strict Content-Security-Policy (`frame-src`/`child-src
  'none'`) can block the on-page preview overlay's iframe from rendering.
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
