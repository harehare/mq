export default defineBackground(() => {
  browser.runtime.onInstalled.addListener(() => {
    browser.sidePanel
      ?.setPanelBehavior({ openPanelOnActionClick: true })
      .catch((error: unknown) => {
        console.error("Failed to set side panel behavior", error);
      });
  });
});
