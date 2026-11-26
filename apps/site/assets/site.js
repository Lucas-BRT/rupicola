(() => {
  const root = document.documentElement;
  const saved = localStorage.getItem("rupicola-theme") || "dark";
  root.setAttribute("data-theme", saved);

  const themeBtn = document.querySelector("[data-theme-toggle]");
  const updateThemeLabel = (theme) => {
    if (!themeBtn) return;
    const labels = { dark: "Dark", light: "Light", system: "System" };
    themeBtn.textContent = labels[theme] || "Dark";
  };
  updateThemeLabel(saved);

  themeBtn?.addEventListener("click", () => {
    const order = ["dark", "light", "system"];
    const curr = root.getAttribute("data-theme") || "dark";
    const next = order[(order.indexOf(curr) + 1) % order.length];
    root.classList.add("theme-switching");
    root.setAttribute("data-theme", next);
    localStorage.setItem("rupicola-theme", next);
    updateThemeLabel(next);
    void root.offsetHeight;
    requestAnimationFrame(() => root.classList.remove("theme-switching"));
  });

  const navToggle = document.querySelector("[data-nav-toggle]");
  const navLinks = document.querySelector("[data-nav-links]");
  navToggle?.addEventListener("click", () => {
    const open = navLinks?.classList.toggle("is-open");
    navToggle.setAttribute("aria-expanded", open ? "true" : "false");
  });

  document.addEventListener("click", async (ev) => {
    const btn = ev.target.closest("[data-copy]");
    if (!btn) return;
    const text = btn.getAttribute("data-copy") || "";
    try {
      await navigator.clipboard.writeText(text);
      const prev = btn.textContent;
      btn.textContent = "Copied";
      setTimeout(() => { btn.textContent = prev; }, 1600);
    } catch {
      btn.textContent = "Failed";
    }
  });

  const filter = document.getElementById("docs-filter");
  if (filter) {
    const items = Array.from(document.querySelectorAll("[data-doc-item]"));
    filter.addEventListener("input", () => {
      const q = filter.value.trim().toLowerCase();
      for (const el of items) {
        const hay = (el.getAttribute("data-doc-item") || "").toLowerCase();
        el.hidden = q.length > 0 && !hay.includes(q);
      }
    });
  }

  // Hero Visual Stage Canvas interactive preview
  const heroDiff = document.getElementById("hero-diff");
  const heroSplit = document.querySelector("[data-hero-split]");
  const heroTitle = document.getElementById("hero-stage-title");
  heroSplit?.addEventListener("input", () => {
    heroDiff?.style.setProperty("--pos", `${heroSplit.value}%`);
  });

  document.querySelectorAll("[data-hero-vp]").forEach((btn) => {
    btn.addEventListener("click", () => {
      const vp = btn.getAttribute("data-hero-vp") || "1440";
      if (heroDiff) {
        heroDiff.setAttribute("data-vp", vp);
        heroDiff.classList.remove("is-settling");
        void heroDiff.offsetWidth;
        heroDiff.classList.add("is-settling");
      }
      document.querySelectorAll("[data-hero-vp]").forEach((b) =>
        b.classList.toggle("btn-primary", b === btn)
      );
    });
  });

  document.querySelector("[data-hero-diff]")?.addEventListener("click", (ev) => {
    const on = heroDiff?.classList.toggle("is-pixel");
    ev.currentTarget.classList.toggle("btn-primary", Boolean(on));
  });

  document.querySelectorAll("[data-hero-tab]").forEach((btn) => {
    btn.addEventListener("click", () => {
      document.querySelectorAll("[data-hero-tab]").forEach((b) =>
        b.classList.toggle("is-active", b === btn)
      );
      const title = btn.getAttribute("data-hero-title");
      if (heroTitle && title) heroTitle.textContent = title;
    });
  });
})();
