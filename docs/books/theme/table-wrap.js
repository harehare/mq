/*
 * Wrap every table rendered in the page content with a scrollable
 * container so wide tables (e.g. the extension modules list) scroll
 * horizontally within their own box on narrow/mobile viewports
 * instead of forcing the whole page to scroll sideways.
 */
(function () {
  function init() {
    const tables = document.querySelectorAll(".content table");

    tables.forEach((table) => {
      if (table.parentElement && table.parentElement.classList.contains("table-wrapper")) {
        return; // Already wrapped
      }

      const wrapper = document.createElement("div");
      wrapper.className = "table-wrapper";
      table.parentNode.insertBefore(wrapper, table);
      wrapper.appendChild(table);
    });
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", init);
  } else {
    init();
  }
})();
