/*
 * Collapse the "Cookbook" section of the sidebar unless the current page is
 * inside it. mdbook's built-in fold option folds every section, so this adds
 * a toggle to the Cookbook entry only and reuses mdbook's toggle styles.
 */
(function () {
  function init() {
    const link = document.querySelector('#sidebar .chapter a[href$="cookbook/index.html"]');
    const item = link && link.parentElement;
    const section = item && item.nextElementSibling;
    if (!section || !section.querySelector("ol.section")) {
      return;
    }

    const inCookbook = link.classList.contains("active") || section.querySelector("a.active");
    if (!inCookbook) {
      item.classList.remove("expanded");
    }

    if (item.querySelector("a.toggle")) {
      return;
    }
    const toggle = document.createElement("a");
    toggle.className = "toggle";
    toggle.innerHTML =
      '<div><svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true"><path d="M3 1l4 4-4 4" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/></svg></div>';
    toggle.setAttribute("aria-label", "Toggle Cookbook section");
    toggle.addEventListener("click", () => item.classList.toggle("expanded"));
    item.appendChild(toggle);
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", init);
  } else {
    init();
  }
})();
