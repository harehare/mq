# Privacy Policy — mq for Markdown (Chrome Extension)

Last updated: 2026-08-10

This privacy policy covers the **mq for Markdown** Chrome extension (`packages/mq-chrome-extension` in this repository).

## Summary

mq for Markdown does not collect, store, or transmit any personal or sensitive user data. All page conversion and query filtering happens locally in your browser, using a local WebAssembly build of mq ([`mq-web`](https://github.com/harehare/mq/tree/main/packages/mq-web)).
Nothing is sent to a server.

## What the extension does

- Reads the HTML of the tab you're currently viewing (only when you click "Extract page") and converts it to Markdown, entirely on your device.
- Runs the mq query you write against that content, also entirely on your device.
- Optionally renders a preview overlay of the converted content on the page you're viewing.
- Copies the result to your clipboard when you click "Copy" — this is a manual, user-initiated action; the extension does not read or write your clipboard otherwise.

## Data collection

We do not collect any data. Specifically, the extension does not:

- Send page content, queries, or results to any external server.
- Use analytics, telemetry, or crash-reporting services.
- Share, sell, or transfer any data to third parties.

## Local storage

The extension uses Chrome's local storage API (`storage` permission) to save only the last mq query you entered, so it's restored the next time you open the side panel. This value stays on your device and is never transmitted anywhere.

## Permissions

| Permission  | Why it's needed                                                                                      |
| ----------- | ---------------------------------------------------------------------------------------------------- |
| `activeTab` | Read the currently open tab's HTML when you click "Extract page".                                    |
| `scripting` | Inject the script that reads the active tab's HTML and renders the optional on-page preview overlay. |
| `sidePanel` | Display the extension's UI in Chrome's side panel.                                                   |
| `storage`   | Persist your last-entered mq query locally between sessions.                                         |

## Changes to this policy

If this policy changes, the updated version will be posted at this same URL with a revised "Last updated" date.

## Contact

Questions or concerns can be filed as an issue at <https://github.com/harehare/mq/issues>.
