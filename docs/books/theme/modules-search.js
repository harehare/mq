/*
 * Search box + category chips for the extension modules table
 * on the "Modules" page (start/modules.md).
 */
(function () {
  function init() {
    const input = document.getElementById("module-search-input");
    const table = document.getElementById("modules-table");
    if (!input || !table) {
      return; // Not on the modules page
    }

    const chipsContainer = document.getElementById("module-category-filters");
    const countEl = document.getElementById("module-count");
    const rows = Array.from(table.querySelectorAll("tbody tr"));
    let activeCategory = "all";

    function applyFilters() {
      const query = input.value.trim().toLowerCase();
      let visible = 0;

      rows.forEach((row) => {
        const matchesCategory =
          activeCategory === "all" || row.dataset.category === activeCategory;
        const matchesQuery =
          !query || row.textContent.toLowerCase().includes(query);
        const show = matchesCategory && matchesQuery;
        row.style.display = show ? "" : "none";
        if (show) visible++;
      });

      if (countEl) {
        countEl.textContent = `${visible} module${visible === 1 ? "" : "s"}`;
      }
    }

    input.addEventListener("input", applyFilters);

    if (chipsContainer) {
      chipsContainer.addEventListener("click", (e) => {
        const chip = e.target.closest("[data-category]");
        if (!chip) return;
        chipsContainer
          .querySelectorAll("[data-category]")
          .forEach((c) => c.classList.remove("active"));
        chip.classList.add("active");
        activeCategory = chip.dataset.category;
        applyFilters();
      });
    }

    applyFilters();
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", init);
  } else {
    init();
  }
})();
