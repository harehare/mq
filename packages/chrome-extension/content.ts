chrome.runtime.onMessage.addListener((request, sender, sendResponse) => {
  if (request.type === 'GET_PAGE_HTML') {
    sendResponse({ html: document.documentElement.outerHTML });
  }
  // Keep the message channel open for sendResponse by returning true,
  // although for this specific simple case it might not be strictly necessary.
  return true;
});
