export default defineBackground(() => {
  // Not using setPanelBehavior({ openPanelOnActionClick: true }): that suppresses
  // onClicked, so re-clicking the icon to re-grant activeTab on a new tab is unreliable.
  browser.action.onClicked.addListener((tab) => {
    if (tab.id === undefined) return;
    browser.sidePanel?.open({ tabId: tab.id }).catch((error: unknown) => {
      console.error("Failed to open side panel", error);
    });
  });
});
