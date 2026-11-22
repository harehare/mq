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
    const chips = chipsContainer
      ? Array.from(chipsContainer.querySelectorAll("[data-category]"))
      : [];
    const chipLabels = new Map(chips.map((c) => [c, c.textContent.trim()]));
    let activeCategory = "all";

    // Keep the list area at its unfiltered height so the page below it
    // does not jump while filtering.
    const wrapper = document.createElement("div");
    table.parentNode.insertBefore(wrapper, table);
    wrapper.appendChild(table);

    const emptyEl = document.createElement("p");
    emptyEl.className = "module-empty";
    emptyEl.textContent = "No modules found.";
    emptyEl.hidden = true;
    wrapper.appendChild(emptyEl);

    function lockHeight() {
      const saved = rows.map((row) => row.style.display);
      wrapper.style.minHeight = "";
      rows.forEach((row) => (row.style.display = ""));
      wrapper.style.minHeight = `${wrapper.offsetHeight}px`;
      rows.forEach((row, i) => (row.style.display = saved[i]));
    }

    // Copy button for the import line, added to each row's name cell.
    rows.forEach((row) => {
      const cell = row.cells[0];
      const name = cell && cell.querySelector("a[href], code");
      if (!name) return;
      // Standard library names are imported by name, extensions by repo path.
      const path = name.href
        ? name.href.replace(/^https?:\/\//, "")
        : name.textContent.trim();
      const button = document.createElement("button");
      button.type = "button";
      button.className = "module-copy";
      button.textContent = "Copy import";
      button.title = `import "${path}"`;
      button.addEventListener("click", async () => {
        try {
          await navigator.clipboard.writeText(`import "${path}"`);
          button.textContent = "Copied";
        } catch (_) {
          button.textContent = "Failed";
        }
        setTimeout(() => (button.textContent = "Copy import"), 1500);
      });
      cell.appendChild(document.createTextNode(" "));
      cell.appendChild(button);
    });

    // Highlight targets: name link and description cell text.
    const targets = rows.map((row) => ({
      row,
      nodes: [row.cells[0].querySelector("a, code"), row.cells[2]].filter(Boolean),
    }));
    const originals = new Map();
    targets.forEach(({ nodes }) =>
      nodes.forEach((n) => originals.set(n, n.innerHTML)),
    );

    function highlight(node, query) {
      node.innerHTML = originals.get(node);
      if (!query) return;
      const walker = document.createTreeWalker(node, NodeFilter.SHOW_TEXT);
      const texts = [];
      while (walker.nextNode()) texts.push(walker.currentNode);
      texts.forEach((t) => {
        const lower = t.nodeValue.toLowerCase();
        let idx = lower.indexOf(query);
        if (idx < 0) return;
        const frag = document.createDocumentFragment();
        let pos = 0;
        while (idx >= 0) {
          frag.appendChild(document.createTextNode(t.nodeValue.slice(pos, idx)));
          const mark = document.createElement("mark");
          mark.textContent = t.nodeValue.slice(idx, idx + query.length);
          frag.appendChild(mark);
          pos = idx + query.length;
          idx = lower.indexOf(query, pos);
        }
        frag.appendChild(document.createTextNode(t.nodeValue.slice(pos)));
        t.parentNode.replaceChild(frag, t);
      });
    }

    function saveState() {
      const params = new URLSearchParams();
      const q = input.value.trim();
      if (q) params.set("q", q);
      if (activeCategory !== "all") params.set("cat", activeCategory);
      const qs = params.toString();
      history.replaceState(
        null,
        "",
        location.pathname + (qs ? `?${qs}` : "") + location.hash,
      );
    }

    function applyFilters() {
      const query = input.value.trim().toLowerCase();
      const counts = new Map();
      let visible = 0;

      targets.forEach(({ row, nodes }) => {
        const matchesQuery =
          !query || row.textContent.toLowerCase().includes(query);
        const matchesCategory =
          activeCategory === "all" || row.dataset.category === activeCategory;
        const show = matchesCategory && matchesQuery;
        row.style.display = show ? "" : "none";
        if (show) visible++;
        if (matchesQuery) {
          counts.set("all", (counts.get("all") || 0) + 1);
          counts.set(
            row.dataset.category,
            (counts.get(row.dataset.category) || 0) + 1,
          );
        }
        nodes.forEach((n) => highlight(n, show ? query : ""));
      });

      chips.forEach((chip) => {
        const n = counts.get(chip.dataset.category) || 0;
        chip.textContent = `${chipLabels.get(chip)} (${n})`;
      });

      emptyEl.hidden = visible > 0;

      if (countEl) {
        countEl.textContent = `${visible} module${visible === 1 ? "" : "s"}`;
      }
      saveState();
    }

    function setCategory(category) {
      activeCategory = category;
      chips.forEach((c) =>
        c.classList.toggle("active", c.dataset.category === category),
      );
    }

    input.addEventListener("input", applyFilters);

    if (chipsContainer) {
      chipsContainer.addEventListener("click", (e) => {
        const chip = e.target.closest("[data-category]");
        if (!chip) return;
        setCategory(chip.dataset.category);
        applyFilters();
      });
    }

    // "/" focuses the search box, Esc clears it.
    document.addEventListener("keydown", (e) => {
      const el = document.activeElement;
      const typing =
        el && (el.tagName === "INPUT" || el.tagName === "TEXTAREA" || el.isContentEditable);
      if (e.key === "/" && !typing && !e.ctrlKey && !e.metaKey && !e.altKey) {
        e.preventDefault();
        input.focus();
        input.select();
      } else if (e.key === "Escape" && el === input) {
        input.value = "";
        applyFilters();
        input.blur();
      }
    });

    const params = new URLSearchParams(location.search);
    input.value = params.get("q") || "";
    const cat = params.get("cat");
    if (cat && chips.some((c) => c.dataset.category === cat)) {
      setCategory(cat);
    }

    applyFilters();
    lockHeight();
    window.addEventListener("load", lockHeight);
    window.addEventListener("resize", lockHeight);
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", init);
  } else {
    init();
  }
})();
