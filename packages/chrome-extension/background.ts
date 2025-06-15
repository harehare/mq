import { Readability } from '@mozilla/readability';
import TurndownService from 'turndown';
import * as mq from 'mq-web'; // Use webpack alias

const turndownService = new TurndownService();

chrome.runtime.onMessage.addListener((request, sender, sendResponse) => {
  if (request.type === 'RUN_MQ_QUERY') {
    const query = request.query;

    chrome.tabs.query({ active: true, currentWindow: true }, (tabs) => {
      if (tabs[0] && tabs[0].id) {
        chrome.tabs.sendMessage(tabs[0].id, { type: 'GET_PAGE_HTML' }, async (response) => {
          if (chrome.runtime.lastError) {
            chrome.runtime.sendMessage({
              type: 'MQ_RESULT',
              error: 'Failed to get page HTML: ' + chrome.runtime.lastError.message,
            });
            return;
          }

          if (response && response.html) {
            try {
              const doc = new DOMParser().parseFromString(response.html, 'text/html');
              const reader = new Readability(doc);
              const article = reader.parse();

              if (article && article.content) { // Check article.content instead of article.textContent
                const markdownContent = turndownService.turndown(article.content);
                const result = await mq.run(query, markdownContent, { inputFormat: 'markdown' });
                chrome.runtime.sendMessage({ type: 'MQ_RESULT', data: result });
              } else {
                chrome.runtime.sendMessage({ type: 'MQ_RESULT', error: 'Could not extract article content using Readability.' });
              }
            } catch (e: any) {
              chrome.runtime.sendMessage({ type: 'MQ_RESULT', error: e.message || 'An unknown error occurred while processing content.' });
            }
          } else {
            chrome.runtime.sendMessage({ type: 'MQ_RESULT', error: 'No HTML content received from content script.' });
          }
        });
      } else {
        chrome.runtime.sendMessage({ type: 'MQ_RESULT', error: 'Could not find active tab.' });
      }
    });
    return true; // Indicates that the response will be sent asynchronously
  }
});
