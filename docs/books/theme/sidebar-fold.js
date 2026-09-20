/*
 * Collapse the "Cookbook" section of the sidebar unless the current page is
 * inside it. mdbook's built-in fold option folds every section, so this adds
 * a toggle to the Cookbook entry only and reuses mdbook's fold styles.
 *
 * Expects the mdbook 0.5 sidebar markup, which toc.js fills in before
 * DOMContentLoaded:
 * <li class="chapter-item expanded">
 *   <span class="chapter-link-wrapper"><a href="cookbook/index.html">...</a></span>
 *   <ol class="section">...</ol>
 * </li>
 */
(function () {
  function init() {
    const link = document.querySelector('#mdbook-sidebar .chapter a[href$="cookbook/index.html"]');
    const wrapper = link && link.parentElement;
    const item = wrapper && wrapper.parentElement;
    if (!item || item.tagName !== "LI" || !item.querySelector(":scope > ol.section")) {
      return;
    }

    const inCookbook = link.classList.contains("active") || item.querySelector("a.active");
    item.classList.toggle("expanded", Boolean(inCookbook));

    if (wrapper.querySelector(".chapter-fold-toggle")) {
      return;
    }
    const toggle = document.createElement("a");
    toggle.className = "chapter-fold-toggle";
    toggle.innerHTML = "<div>❱</div>";
    toggle.setAttribute("aria-label", "Toggle Cookbook section");
    toggle.addEventListener("click", () => item.classList.toggle("expanded"));
    wrapper.appendChild(toggle);
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", init);
  } else {
    init();
  }
})();
