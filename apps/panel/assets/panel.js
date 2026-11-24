const dateFormat = new Intl.DateTimeFormat(undefined, { dateStyle: "medium", timeStyle: "medium" });
for (const label of document.querySelectorAll("[data-epoch-ms]")) {
  const date = new Date(Number(label.dataset.epochMs));
  if (!Number.isNaN(date.getTime())) {
    const suffix = label.dataset.timeSuffix;
    label.textContent = suffix ? `${dateFormat.format(date)} · ${suffix}` : dateFormat.format(date);
  }
}

for (const marker of document.querySelectorAll("[data-seek]")) {
  const video = marker.closest("article").querySelector("video");
  marker.addEventListener("click", () => {
    video.currentTime = Number(marker.dataset.seek);
    video.focus();
  });
}

const preferences = ["dark", "light", "system"];
const themeChoice = document.querySelector("[data-theme-choice]");
const themeToggleBtn = document.querySelector("[data-theme-toggle]");
const syncThemeToggle = (mode) => {
  if (!themeToggleBtn) return;
  themeToggleBtn.textContent = mode === "light" ? "Dark" : "Light";
};
try {
  const saved = localStorage.getItem("rupicola-theme") || "dark";
  if (preferences.includes(saved)) {
    document.documentElement.dataset.theme = saved;
    if (themeChoice) themeChoice.value = saved;
    syncThemeToggle(saved);
  }
  themeChoice?.addEventListener("change", () => {
    if (!preferences.includes(themeChoice.value)) return;
    document.documentElement.dataset.theme = themeChoice.value;
    localStorage.setItem("rupicola-theme", themeChoice.value);
    syncThemeToggle(themeChoice.value);
  });
  themeToggleBtn?.addEventListener("click", () => {
    const curr = document.documentElement.dataset.theme || "dark";
    const next = curr === "light" ? "dark" : "light";
    document.documentElement.dataset.theme = next;
    localStorage.setItem("rupicola-theme", next);
    if (themeChoice) themeChoice.value = next;
    syncThemeToggle(next);
  });
} catch {
  if (themeChoice) themeChoice.disabled = true;
}

// Visual Stage Canvas: viewport HUD, collapsible inspector rail, and filmstrip jump
const stageCanvas = document.querySelector("[data-stage-canvas]");
const stageShell = document.querySelector("[data-stage-shell]");
const railToggleBtn = document.querySelector("[data-rail-toggle]");
for (const vpBtn of document.querySelectorAll("[data-stage-vp]")) {
  vpBtn.addEventListener("click", () => {
    const vp = vpBtn.dataset.stageVp || "100%";
    stageCanvas?.style.setProperty("--stage-max", vp);
    for (const b of document.querySelectorAll("[data-stage-vp]")) {
      b.classList.toggle("primary", b === vpBtn);
    }
  });
}
railToggleBtn?.addEventListener("click", () => {
  const hidden = stageShell?.classList.toggle("is-rail-hidden");
  railToggleBtn.textContent = hidden ? "Show Inspector" : "Hide Inspector";
  railToggleBtn.setAttribute("aria-expanded", hidden ? "false" : "true");
});
for (const thumb of document.querySelectorAll("[data-film-pick]")) {
  thumb.addEventListener("click", () => {
    for (const t of document.querySelectorAll("[data-film-pick]")) {
      t.classList.toggle("is-active", t === thumb);
    }
  });
}

document.querySelector("[data-form-error]")?.focus();

const viewer = document.querySelector("#image-viewer");
const thumbnails = [...document.querySelectorAll("[data-viewer]")];
if (viewer && typeof viewer.showModal === "function") {
  const title = viewer.querySelector("h2");
  const picture = viewer.querySelector("img");
  const original = viewer.querySelector("[data-image-original]");
  const previous = viewer.querySelector("[data-image-prev]");
  const next = viewer.querySelector("[data-image-next]");
  let current = 0;
  let opener;

  function show(index) {
    current = index;
    const thumbnail = thumbnails[index];
    title.textContent = thumbnail.dataset.viewer;
    picture.alt = thumbnail.querySelector("img").alt;
    picture.src = thumbnail.href;
    original.href = thumbnail.href;
    const focused = document.activeElement;
    previous.disabled = current === 0;
    next.disabled = current === thumbnails.length - 1;
    if (focused === previous && previous.disabled) {
      (!next.disabled ? next : viewer.querySelector("button:not([disabled])"))?.focus();
    }
    if (focused === next && next.disabled) {
      (!previous.disabled ? previous : viewer.querySelector("button:not([disabled])"))?.focus();
    }
  }

  thumbnails.forEach((thumbnail, index) => {
    thumbnail.addEventListener("click", event => {
      if (event.ctrlKey || event.metaKey || event.shiftKey || event.altKey) return;
      event.preventDefault();
      opener = thumbnail;
      show(index);
      viewer.showModal();
    });
  });
  previous.addEventListener("click", () => show(current - 1));
  next.addEventListener("click", () => show(current + 1));
  viewer.addEventListener("keydown", event => {
    if (event.key === "ArrowLeft" && !previous.disabled) show(current - 1);
    if (event.key === "ArrowRight" && !next.disabled) show(current + 1);
  });
  viewer.addEventListener("close", () => opener?.focus());
}

const projectNav = document.querySelector(".tabs");
const currentPage = projectNav?.querySelector('[aria-current="page"]');
if (currentPage && projectNav.scrollWidth > projectNav.clientWidth) {
  const nav = projectNav.getBoundingClientRect();
  const active = currentPage.getBoundingClientRect();
  projectNav.scrollLeft += Math.max(0, active.right - nav.right + 6);
}

const proposalModal = document.querySelector("#proposal-modal");
if (proposalModal && typeof proposalModal.showModal === "function") {
  const modalTitle = proposalModal.querySelector("[data-proposal-title]");
  const modalBody = proposalModal.querySelector("[data-proposal-body]");
  let proposalOpener;
  for (const btn of document.querySelectorAll("[data-proposal-open]")) {
    btn.addEventListener("click", () => {
      const id = btn.dataset.proposalOpen;
      const tpl = document.querySelector(`[data-proposal-template="${id}"]`);
      const card = btn.closest("[data-proposal-card]");
      if (!tpl) return;
      proposalOpener = btn;
      modalTitle.textContent = card?.querySelector("h2")?.textContent ?? "Proposal";
      modalBody.replaceChildren(tpl.content.cloneNode(true));
      for (const vp of modalBody.querySelectorAll("[data-vp]")) {
        vp.addEventListener("click", () => {
          const iframe = modalBody.querySelector(".proposal-iframe");
          if (iframe) iframe.style.inlineSize = vp.dataset.vp;
        });
      }
      const copyBtn = modalBody.querySelector("[data-copy-prompt]");
      copyBtn?.addEventListener("click", async () => {
        const text = modalBody.querySelector(".structured-prompt")?.textContent ?? "";
        try {
          await navigator.clipboard.writeText(text);
          copyBtn.textContent = "Copied";
        } catch {}
      });
      proposalModal.showModal();
    });
  }
  proposalModal.addEventListener("close", () => {
    modalBody.replaceChildren();
    proposalOpener?.focus();
  });
}

for (const comp of document.querySelectorAll("[data-comparator]")) {
  const viewport = comp.querySelector(".split-viewport");
  const range = comp.querySelector("[data-split-range]");
  const output = comp.querySelector("[data-split-output]");

  function setSplit(pct) {
    const clamped = Math.max(0, Math.min(100, Math.round(pct)));
    if (viewport) viewport.style.setProperty("--split", `${clamped}%`);
    if (range) range.value = String(clamped);
    if (output) output.textContent = `${clamped}%`;
  }

  range?.addEventListener("input", () => setSplit(Number(range.value)));

  if (viewport) {
    let dragging = false;
    const updateFromPointer = (e) => {
      const rect = viewport.getBoundingClientRect();
      if (rect.width <= 0) return;
      setSplit(((e.clientX - rect.left) / rect.width) * 100);
    };
    viewport.addEventListener("pointerdown", (e) => {
      dragging = true;
      viewport.setPointerCapture?.(e.pointerId);
      updateFromPointer(e);
    });
    viewport.addEventListener("pointermove", (e) => {
      if (dragging) updateFromPointer(e);
    });
    viewport.addEventListener("pointerup", () => {
      dragging = false;
    });
  }

  const modeBtns = [...comp.querySelectorAll("[data-compare-mode]")];
  const stages = [...comp.querySelectorAll("[data-stage-mode]")];
  for (const btn of modeBtns) {
    btn.addEventListener("click", () => {
      const mode = btn.dataset.compareMode;
      for (const b of modeBtns) {
        b.classList.toggle("primary", b === btn);
      }
      for (const st of stages) {
        st.hidden = st.dataset.stageMode !== mode;
      }
    });
  }

  const pairBtns = [...comp.querySelectorAll("[data-pair-before]")];
  const isVid = (u) => {
    const clean = String(u || "").split("?")[0];
    return clean.endsWith(".mp4") || clean.endsWith(".webm");
  };
  for (const btn of pairBtns) {
    btn.addEventListener("click", () => {
      for (const b of pairBtns) b.classList.toggle("primary", b === btn);
      const before = btn.dataset.pairBefore || "";
      const after = btn.dataset.pairAfter || "";
      const diff = btn.dataset.pairDiff || "";
      const beforeIsVid = isVid(before);
      const afterIsVid = isVid(after);

      for (const img of comp.querySelectorAll("[data-img-before], [data-side-before]")) {
        img.hidden = beforeIsVid;
        if (!beforeIsVid && before) img.src = before;
      }
      for (const vid of comp.querySelectorAll("[data-vid-before], [data-side-vid-before]")) {
        vid.hidden = !beforeIsVid;
        if (beforeIsVid && before) vid.src = before;
      }
      for (const img of comp.querySelectorAll("[data-img-after], [data-side-after]")) {
        img.hidden = afterIsVid;
        if (!afterIsVid && after) img.src = after;
      }
      for (const vid of comp.querySelectorAll("[data-vid-after], [data-side-vid-after], [data-full-video]")) {
        vid.hidden = !afterIsVid;
        if (afterIsVid && after) vid.src = after;
      }
      for (const img of comp.querySelectorAll("[data-diff-img]")) {
        if (diff) img.src = diff;
      }
    });
  }

  // Synchronized video transport controls (Split Slider + Side-by-Side + Full Video)
  const transport = comp.querySelector("[data-video-transport]");
  if (transport) {
    const toggleBtn = transport.querySelector("[data-video-toggle]");
    const restartBtn = transport.querySelector("[data-video-restart]");
    const scrub = transport.querySelector("[data-video-scrub]");
    const timeOut = transport.querySelector("[data-video-time]");
    const rateBtns = [...transport.querySelectorAll("[data-video-rate]")];
    const activeVideos = () =>
      [...comp.querySelectorAll("video")].filter((v) => !v.hasAttribute("hidden") && v.getAttribute("src"));

    toggleBtn?.addEventListener("click", () => {
      const vids = activeVideos();
      if (!vids.length) return;
      const shouldPause = !vids[0].paused;
      for (const v of vids) {
        if (shouldPause) v.pause();
        else v.play().catch(() => {});
      }
      toggleBtn.textContent = shouldPause ? "Play ▶" : "Pause ❚❚";
    });

    restartBtn?.addEventListener("click", () => {
      for (const v of activeVideos()) {
        v.currentTime = 0;
        v.play().catch(() => {});
      }
      if (toggleBtn) toggleBtn.textContent = "Pause ❚❚";
    });

    for (const rBtn of rateBtns) {
      rBtn.addEventListener("click", () => {
        const rate = Number(rBtn.dataset.videoRate) || 1;
        for (const b of rateBtns) b.classList.toggle("primary", b === rBtn);
        for (const v of activeVideos()) {
          v.playbackRate = rate;
        }
      });
    }

    scrub?.addEventListener("input", () => {
      const ratio = Number(scrub.value) / 1000;
      for (const v of activeVideos()) {
        if (Number.isFinite(v.duration) && v.duration > 0) {
          v.currentTime = ratio * v.duration;
        }
      }
    });

    const primaryVid = activeVideos()[0];
    primaryVid?.addEventListener("timeupdate", () => {
      if (!Number.isFinite(primaryVid.duration) || primaryVid.duration <= 0) return;
      if (scrub) scrub.value = String(Math.round((primaryVid.currentTime / primaryVid.duration) * 1000));
      if (timeOut) timeOut.textContent = `${primaryVid.currentTime.toFixed(1)}s / ${primaryVid.duration.toFixed(1)}s`;
    });
  }
}
