const dateFormat = new Intl.DateTimeFormat(undefined, { dateStyle: "medium", timeStyle: "medium" });

let activeJobsTimer = null;

function positionRailIndicator(rail, immediate = false) {
  if (!rail) return;
  const indicator = rail.querySelector(".rail-indicator");
  const active = rail.querySelector('a[aria-current="page"]');
  if (!indicator || !active) return;
  if (immediate) rail.classList.add("no-motion");
  indicator.style.transform = `translate3d(${active.offsetLeft}px, ${active.offsetTop}px, 0)`;
  rail.classList.add("has-indicator");
  if (immediate) {
    void indicator.offsetWidth;
    rail.classList.remove("no-motion");
  }
}

function hydrateMain(root = document) {
  for (const label of root.querySelectorAll("[data-epoch-ms]")) {
    const date = new Date(Number(label.dataset.epochMs));
    if (!Number.isNaN(date.getTime())) {
      const suffix = label.dataset.timeSuffix;
      label.textContent = suffix ? `${dateFormat.format(date)} · ${suffix}` : dateFormat.format(date);
    }
  }

  for (const marker of root.querySelectorAll("[data-seek]")) {
    const video = marker.closest("article")?.querySelector("video");
    if (!video) continue;
    marker.addEventListener("click", () => {
      video.currentTime = Number(marker.dataset.seek);
      video.focus();
    });
  }

  // Generic modal triggers (<dialog class="panel-modal">)
  for (const btn of root.querySelectorAll("[data-modal-open]")) {
    btn.addEventListener("click", () => {
      const modal = document.getElementById(btn.dataset.modalOpen);
      if (!modal || typeof modal.showModal !== "function") return;
      modal._opener = btn;
      modal.showModal();
    });
  }
  for (const modal of root.querySelectorAll("dialog.panel-modal")) {
    modal.addEventListener("click", (event) => {
      if (event.target === modal) modal.close();
    });
    modal.addEventListener("close", () => modal._opener?.focus());
    if (modal.hasAttribute("data-auto-open") && !modal.open && typeof modal.showModal === "function") {
      modal.showModal();
    }
  }

  // Visual Stage Canvas: viewport HUD, collapsible inspector rail, and filmstrip jump
  const stageCanvas = root.querySelector("[data-stage-canvas]");
  const stageShell = root.querySelector("[data-stage-shell]");
  const railToggleBtn = root.querySelector("[data-rail-toggle]");
  for (const vpBtn of root.querySelectorAll("[data-stage-vp]")) {
    vpBtn.addEventListener("click", () => {
      const vp = vpBtn.dataset.stageVp || "100%";
      stageCanvas?.style.setProperty("--stage-max", vp);
      for (const b of root.querySelectorAll("[data-stage-vp]")) {
        b.classList.toggle("primary", b === vpBtn);
      }
    });
  }
  railToggleBtn?.addEventListener("click", () => {
    const hidden = stageShell?.classList.toggle("is-rail-hidden");
    const hideLabel = railToggleBtn.dataset.labelHide || "Hide Inspector";
    const showLabel = railToggleBtn.dataset.labelShow || "Show Inspector";
    railToggleBtn.textContent = hidden ? showLabel : hideLabel;
    railToggleBtn.setAttribute("aria-expanded", hidden ? "false" : "true");
  });
  for (const thumb of root.querySelectorAll("[data-film-pick]")) {
    thumb.addEventListener("click", () => {
      for (const t of root.querySelectorAll("[data-film-pick]")) {
        t.classList.toggle("is-active", t === thumb);
      }
    });
  }

  root.querySelector("[data-form-error]")?.focus();

  const viewer = root.querySelector("#image-viewer");
  const thumbnails = [...root.querySelectorAll("[data-viewer]")];
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

  const projectNav = root.querySelector(".tabs");
  const currentPage = projectNav?.querySelector('[aria-current="page"]');
  if (currentPage && projectNav.scrollWidth > projectNav.clientWidth) {
    const nav = projectNav.getBoundingClientRect();
    const active = currentPage.getBoundingClientRect();
    projectNav.scrollLeft += Math.max(0, active.right - nav.right + 6);
  }

  const proposalModal = root.querySelector("#proposal-modal");
  if (proposalModal && typeof proposalModal.showModal === "function") {
    const modalTitle = proposalModal.querySelector("[data-proposal-title]");
    const modalBody = proposalModal.querySelector("[data-proposal-body]");
    let proposalOpener;
    for (const btn of root.querySelectorAll("[data-proposal-open]")) {
      btn.addEventListener("click", () => {
        const id = btn.dataset.proposalOpen;
        const tpl = root.querySelector(`[data-proposal-template="${id}"]`);
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

  for (const opener of root.querySelectorAll("[data-drawer-open]")) {
    opener.addEventListener("click", () => {
      const targetId = opener.dataset.drawerOpen;
      const drawer = targetId ? document.getElementById(targetId) : null;
      if (drawer && typeof drawer.showModal === "function") {
        drawer.showModal();
      }
    });
  }
  for (const drawer of root.querySelectorAll("[data-details-drawer]")) {
    drawer.querySelector("[data-drawer-close]")?.addEventListener("click", () => drawer.close());
    drawer.addEventListener("click", (e) => {
      if (e.target === drawer) drawer.close();
    });
  }

  for (const comp of root.querySelectorAll("[data-comparator]")) {
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
        for (const b of modeBtns) b.classList.toggle("primary", b === btn);
        for (const st of stages) st.hidden = st.dataset.stageMode !== mode;
      });
    }

    const zoomBtns = [...comp.querySelectorAll("[data-compare-zoom]")];
    for (const btn of zoomBtns) {
      btn.addEventListener("click", () => {
        const zoom = btn.dataset.compareZoom || "fit";
        comp.dataset.zoom = zoom;
        for (const b of zoomBtns) b.classList.toggle("primary", b === btn);
      });
    }

    const fsBtn = comp.querySelector("[data-compare-fullscreen]");
    if (fsBtn) {
      const setFullscreen = (open) => {
        comp.classList.toggle("is-fullscreen", open);
        document.body.classList.toggle("has-fullscreen-compare", open);
        fsBtn.textContent = open ? "Exit fullscreen" : "Fullscreen";
        fsBtn.setAttribute("aria-pressed", open ? "true" : "false");
      };
      fsBtn.addEventListener("click", () => setFullscreen(!comp.classList.contains("is-fullscreen")));
      document.addEventListener("keydown", (e) => {
        if (e.key === "Escape" && comp.classList.contains("is-fullscreen")) {
          setFullscreen(false);
          fsBtn.focus();
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
          for (const v of activeVideos()) v.playbackRate = rate;
        });
      }

      scrub?.addEventListener("input", () => {
        const ratio = Number(scrub.value) / 1000;
        for (const v of activeVideos()) {
          if (Number.isFinite(v.duration) && v.duration > 0) v.currentTime = ratio * v.duration;
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

  if (activeJobsTimer) {
    clearTimeout(activeJobsTimer);
    activeJobsTimer = null;
  }
  const jobsIndicator = document.querySelector("[data-jobs-url]");
  if (jobsIndicator) {
    const jobsUrl = jobsIndicator.dataset.jobsUrl;
    const busyPollMs = 4000;
    const idlePollMs = 10000;
    const pollJobs = async () => {
      let open = 0;
      try {
        const resp = await fetch(jobsUrl, { headers: { Accept: "application/json" } });
        if (resp.ok) {
          const jobs = await resp.json();
          open = jobs.length;
          const done = jobs.reduce((acc, j) => acc + (j.cells_done || 0), 0);
          const total = jobs.reduce((acc, j) => acc + (j.cells_total || 0), 0);
          const noun = jobs.length === 1 ? "capture job" : "capture jobs";
          const verb = jobs.some((j) => j.status === "running") ? "running" : "queued";
          jobsIndicator.textContent = `${jobs.length} ${noun} ${verb} · ${done}/${total} cells`;
          jobsIndicator.hidden = open === 0;
          for (const badge of document.querySelectorAll("[data-fix-job-badge]")) {
            const job = jobs.find((j) => j.fix === badge.dataset.fixJobBadge);
            badge.hidden = !job;
            if (job) {
              badge.textContent =
                job.status === "queued"
                  ? "Queued for recapture"
                  : `Recapturing · ${job.cells_done}/${job.cells_total}`;
            }
          }
        }
      } catch {}
      activeJobsTimer = setTimeout(pollJobs, open > 0 ? busyPollMs : idlePollMs);
    };
    activeJobsTimer = setTimeout(pollJobs, jobsIndicator.hidden ? idlePollMs : busyPollMs);
  }

  enhanceNativeSelects(root);
  for (const seg of root.querySelectorAll("[data-segmented]")) {
    positionSegmentedIndicator(seg, true);
  }
}

function positionSegmentedIndicator(seg, immediate = false) {
  if (!seg) return;
  const indicator = seg.querySelector(".ui-segmented-indicator");
  const active = seg.querySelector('a[aria-current="page"]');
  if (!indicator || !active) return;
  const segRect = seg.getBoundingClientRect();
  const actRect = active.getBoundingClientRect();
  if (!actRect.width || !actRect.height) return;
  const x = actRect.left - segRect.left - seg.clientLeft;
  const y = actRect.top - segRect.top - seg.clientTop;
  if (immediate) seg.classList.add("no-motion");
  indicator.style.inlineSize = `${actRect.width}px`;
  indicator.style.blockSize = `${actRect.height}px`;
  indicator.style.transform = `translate3d(${x}px, ${y}px, 0)`;
  seg.classList.add("has-indicator");
  if (immediate) {
    void indicator.offsetWidth;
    seg.classList.remove("no-motion");
  }
}

function setDropdownOpen(dd, open) {
  dd.classList.toggle("is-open", open);
  dd.querySelector(".ui-dropdown-trigger")?.setAttribute("aria-expanded", open ? "true" : "false");
  const menu = dd.querySelector(".ui-dropdown-menu");
  if (menu) {
    menu.setAttribute("aria-hidden", open ? "false" : "true");
    menu.inert = !open;
  }
}

function closeAllDropdowns(except = null) {
  for (const dd of document.querySelectorAll("[data-dropdown].is-open")) {
    if (dd === except) continue;
    setDropdownOpen(dd, false);
  }
}

function selectDropdownOption(dd, optionBtn) {
  const value = optionBtn.dataset.value ?? "";
  const label = optionBtn.querySelector("span")?.textContent ?? optionBtn.textContent ?? "";
  const hiddenInput = dd.querySelector('input[type="hidden"]');
  const nativeSelect = dd.querySelector("select.ui-native-select-hidden");
  const labelEl = dd.querySelector("[data-dropdown-label]");
  const changed = (hiddenInput ? hiddenInput.value : nativeSelect?.value) !== value;
  if (labelEl) {
    labelEl.textContent = label;
    if (changed) {
      labelEl.classList.remove("is-changing");
      void labelEl.offsetWidth;
      labelEl.classList.add("is-changing");
    }
  }
  for (const opt of dd.querySelectorAll("[data-dropdown-option]")) {
    opt.setAttribute("aria-selected", opt === optionBtn ? "true" : "false");
  }
  if (hiddenInput) hiddenInput.value = value;
  if (nativeSelect) nativeSelect.value = value;
  closeAllDropdowns();
  dd.querySelector(".ui-dropdown-trigger")?.focus();
  if (changed) {
    const target = hiddenInput || nativeSelect;
    target?.dispatchEvent(new Event("change", { bubbles: true }));
    if (dd.hasAttribute("data-auto-submit")) {
      const form = dd.closest("form");
      if (form) form.requestSubmit();
    }
  }
}

function enhanceNativeSelects(root = document) {
  for (const select of root.querySelectorAll("select:not(.ui-native-select-hidden)")) {
    if (select.closest("[data-dropdown]")) continue;
    const wrapper = document.createElement("div");
    wrapper.className = "ui-dropdown";
    wrapper.setAttribute("data-dropdown", "");
    if (select.hasAttribute("data-auto-submit")) wrapper.setAttribute("data-auto-submit", "");

    const trigger = document.createElement("button");
    trigger.type = "button";
    trigger.className = "ui-dropdown-trigger";
    trigger.setAttribute("aria-haspopup", "listbox");
    trigger.setAttribute("aria-expanded", "false");
    const selectedOpt = select.options[select.selectedIndex] || select.options[0];
    trigger.innerHTML = `<span class="ui-dropdown-value" data-dropdown-label=""></span><svg class="ui-dropdown-chevron" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="m4 6 4 4 4-4"/></svg>`;
    trigger.querySelector("[data-dropdown-label]").textContent = selectedOpt?.textContent || "";

    const menu = document.createElement("ul");
    menu.className = "ui-dropdown-menu";
    menu.setAttribute("role", "listbox");
    menu.setAttribute("aria-hidden", "true");
    menu.inert = true;
    menu.tabIndex = -1;
    for (const opt of select.options) {
      const li = document.createElement("li");
      li.setAttribute("role", "presentation");
      const btn = document.createElement("button");
      btn.type = "button";
      btn.className = "ui-dropdown-option";
      btn.setAttribute("role", "option");
      btn.setAttribute("data-dropdown-option", "");
      btn.dataset.value = opt.value;
      btn.setAttribute("aria-selected", opt.selected ? "true" : "false");
      btn.innerHTML = `<span></span><svg class="ui-dropdown-check" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="m3.5 8.5 3 3 6-6.5"/></svg>`;
      btn.querySelector("span").textContent = opt.textContent || "";
      li.appendChild(btn);
      menu.appendChild(li);
    }

    select.classList.add("ui-native-select-hidden");
    select.tabIndex = -1;
    select.parentNode.insertBefore(wrapper, select);
    wrapper.appendChild(select);
    wrapper.appendChild(trigger);
    wrapper.appendChild(menu);
  }
}

function syncDropdownValue(inputEl, value) {
  const dd = inputEl?.closest("[data-dropdown]");
  if (!dd) return;
  const opt = [...dd.querySelectorAll("[data-dropdown-option]")].find((o) => o.dataset.value === value);
  if (!opt) return;
  const labelEl = dd.querySelector("[data-dropdown-label]");
  if (labelEl) labelEl.textContent = opt.querySelector("span")?.textContent ?? "";
  for (const o of dd.querySelectorAll("[data-dropdown-option]")) {
    o.setAttribute("aria-selected", o === opt ? "true" : "false");
  }
}

hydrateMain(document);
positionRailIndicator(document.querySelector("[data-project-rail]"), true);
window.addEventListener("resize", () => {
  positionRailIndicator(document.querySelector("[data-project-rail]"), true);
  for (const seg of document.querySelectorAll("[data-segmented]")) {
    positionSegmentedIndicator(seg, true);
  }
});

// Partial SSR navigation with Topcoat Suspense NDJSON streaming + View Transitions
let navAbort = null;
let swapAbort = null;
const SWAP_CACHE_TTL_MS = 60_000;
const SWAP_CACHE_MAX = 40;
const swapCache = new Map();

function swapCacheKey(targetId, url) {
  return `${targetId}::${url.pathname}${url.search}`;
}

function swapCacheGet(key) {
  const entry = swapCache.get(key);
  if (!entry) return null;
  if (Date.now() - entry.ts > SWAP_CACHE_TTL_MS) {
    swapCache.delete(key);
    return null;
  }
  swapCache.delete(key);
  swapCache.set(key, entry);
  return entry.html;
}

function swapCacheSet(key, html) {
  swapCache.delete(key);
  swapCache.set(key, { html, ts: Date.now() });
  if (swapCache.size > SWAP_CACHE_MAX) {
    const oldest = swapCache.keys().next().value;
    if (oldest !== undefined) swapCache.delete(oldest);
  }
}

function applyTopcoatSwapIn(doc, regionId, html, animate) {
  const startText = `::topcoat::region::start(${regionId})`;
  const endText = `::topcoat::region::end(${regionId})`;
  const rootNode = doc.body || doc;
  const walker = doc.createTreeWalker(rootNode, NodeFilter.SHOW_COMMENT);
  let startNode = null;
  let endNode = null;
  for (let node = walker.nextNode(); node; node = walker.nextNode()) {
    if (node.data === startText) startNode = node;
    else if (node.data === endText) {
      endNode = node;
      break;
    }
  }
  if (!startNode || !endNode || startNode.parentNode !== endNode.parentNode) return;
  const parent = startNode.parentNode;
  const range = doc.createRange();
  range.setStartAfter(startNode);
  range.setEndBefore(endNode);
  range.deleteContents();
  const fragment = range.createContextualFragment(html);
  const reducedMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  const inserted = [...fragment.children];
  range.insertNode(fragment);
  if (animate && !reducedMotion) {
    for (const el of inserted) {
      el.classList.add("is-hydrating");
      el.addEventListener("animationend", () => el.classList.remove("is-hydrating"), { once: true });
    }
  }
  if (animate) hydrateMain(parent);
}

function applyTopcoatSwap(regionId, html) {
  applyTopcoatSwapIn(document, regionId, html, true);
}

function buildSwapSkeleton(kind) {
  const wrap = document.createElement("div");
  wrap.setAttribute("aria-busy", "true");
  wrap.setAttribute("aria-label", "Loading updated results");
  if (kind === "cards") {
    wrap.className = "swap-suspense-cards";
    for (let i = 0; i < 6; i++) {
      const card = document.createElement("div");
      card.className = "skel-card";
      card.innerHTML = `<span class="skel skel-title"></span><span class="skel skel-sm"></span><span class="skel skel-row"></span>`;
      wrap.appendChild(card);
    }
    return wrap;
  }
  wrap.className = "swap-suspense";
  for (let i = 0; i < 5; i++) {
    const row = document.createElement("div");
    row.className = "swap-skel-row";
    row.innerHTML = `<span class="skel swap-skel-dot"></span><span class="swap-skel-lines"><span class="skel skel-title"></span><span class="skel skel-sm"></span></span><span class="skel swap-skel-time"></span>`;
    wrap.appendChild(row);
  }
  return wrap;
}

function applyStagger(container) {
  const items = container.querySelectorAll(
    ".dash-run-list > li, .run-list > li, .project-index > .project-card, .problem-queue > *, .fixes-list > *",
  );
  items.forEach((el, idx) => {
    el.style.setProperty("--stagger", String(Math.min(idx, 6)));
  });
}

function commitSwappedRegion(currEl, nextEl, url, targetId, pushHistory, reducedMotion) {
  const currBody = currEl.querySelector("[data-swap-body]");
  const nextBody = nextEl.querySelector("[data-swap-body]");
  if (currBody && nextBody) {
    for (const nextMeta of nextEl.querySelectorAll("[data-swap-meta]")) {
      const key = nextMeta.dataset.swapMeta;
      const currMeta = currEl.querySelector(`[data-swap-meta="${key}"]`);
      if (!currMeta) continue;
      if (currMeta.tagName === "INPUT") currMeta.value = nextMeta.value;
      else currMeta.textContent = nextMeta.textContent;
    }
    const currSeg = currEl.querySelector("[data-segmented]");
    const nextSeg = nextEl.querySelector("[data-segmented]");
    if (currSeg && nextSeg) {
      const currLinks = [...currSeg.querySelectorAll("a")];
      const nextLinks = [...nextSeg.querySelectorAll("a")];
      currLinks.forEach((a, idx) => {
        const n = nextLinks[idx];
        if (!n) return;
        a.href = n.getAttribute("href") || a.href;
        if (n.getAttribute("aria-current") === "page") a.setAttribute("aria-current", "page");
        else a.removeAttribute("aria-current");
      });
      positionSegmentedIndicator(currSeg, false);
    }
    for (const nextInput of nextEl.querySelectorAll('[data-dropdown] input[type="hidden"]')) {
      const currInput = currEl.querySelector(`#${CSS.escape(nextInput.id)}`);
      if (currInput && currInput.value !== nextInput.value) {
        currInput.value = nextInput.value;
        syncDropdownValue(currInput, nextInput.value);
      }
    }
    currBody.style.minBlockSize = "";
    currBody.removeAttribute("aria-busy");
    currBody.replaceChildren(...nextBody.childNodes);
    applyStagger(currBody);
    if (!reducedMotion) {
      currBody.classList.remove("is-region-updated");
      void currBody.offsetWidth;
      currBody.classList.add("is-region-updated");
      currBody.addEventListener("animationend", () => currBody.classList.remove("is-region-updated"), { once: true });
    }
    hydrateMain(currBody);
  } else {
    currEl.replaceWith(nextEl);
    if (!reducedMotion) {
      nextEl.classList.add("is-region-updated");
      nextEl.addEventListener("animationend", () => nextEl.classList.remove("is-region-updated"), { once: true });
    }
    hydrateMain(nextEl);
  }
  if (pushHistory && url.href !== location.href) {
    history.pushState({ swapTarget: targetId }, "", url.href);
  }
}

async function swapRegion(href, targetId, pushHistory = true) {
  const url = new URL(href, location.href);
  if (url.origin !== location.origin) {
    location.href = href;
    return;
  }
  const currEl = document.getElementById(targetId);
  if (!currEl) {
    location.href = url.href;
    return;
  }

  // Memoize current initial region before leaving it so returning is instant.
  const currUrl = new URL(location.href);
  const currKey = swapCacheKey(targetId, currUrl);
  const currBody = currEl.querySelector("[data-swap-body]");
  if (!swapCache.has(currKey) && currBody?.getAttribute("aria-busy") !== "true") {
    swapCacheSet(currKey, currEl.outerHTML);
  }

  const reducedMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  const nextKey = swapCacheKey(targetId, url);
  const cachedHtml = swapCacheGet(nextKey);
  if (cachedHtml) {
    swapAbort?.abort();
    const doc = new DOMParser().parseFromString(cachedHtml, "text/html");
    const cachedEl = doc.getElementById(targetId);
    if (cachedEl) {
      commitSwappedRegion(currEl, cachedEl, url, targetId, pushHistory, reducedMotion);
      return;
    }
  }

  swapAbort?.abort();
  swapAbort = new AbortController();

  const startedAt = performance.now();
  const minSuspenseMs = reducedMotion ? 0 : 150;

  if (currBody) {
    const prevHeight = currBody.getBoundingClientRect().height;
    if (prevHeight > 80) currBody.style.minBlockSize = `${Math.round(prevHeight)}px`;
    currBody.replaceChildren(buildSwapSkeleton(currBody.dataset.swapBody || "rows"));
    currBody.setAttribute("aria-busy", "true");
  }

  try {
    const resp = await fetch(url.href, {
      headers: { Accept: "application/x-ndjson, text/html;q=0.9" },
      credentials: "same-origin",
      signal: swapAbort.signal,
    });
    if (!resp.ok) {
      location.href = url.href;
      return;
    }
    const contentType = resp.headers.get("content-type") || "";
    let parsedDoc = null;
    if (contentType.includes("application/x-ndjson") && resp.body) {
      const text = await resp.text();
      for (const line of text.split("\n")) {
        if (!line.trim()) continue;
        const frame = JSON.parse(line);
        if (frame.t === "snapshot") {
          parsedDoc = new DOMParser().parseFromString(frame.html, "text/html");
        } else if (frame.t === "swap" && parsedDoc) {
          applyTopcoatSwapIn(parsedDoc, frame.region, frame.html, false);
        } else if (frame.t === "redirect") {
          location.href = frame.location;
          return;
        }
      }
    } else if (contentType.includes("text/html")) {
      parsedDoc = new DOMParser().parseFromString(await resp.text(), "text/html");
    }
    const nextEl = parsedDoc?.getElementById(targetId);
    if (!nextEl) {
      location.href = url.href;
      return;
    }

    swapCacheSet(nextKey, nextEl.outerHTML);

    const elapsed = performance.now() - startedAt;
    if (currBody && elapsed < minSuspenseMs) {
      await new Promise((resolve) => setTimeout(resolve, minSuspenseMs - elapsed));
      if (swapAbort.signal.aborted) return;
    }

    commitSwappedRegion(currEl, nextEl, url, targetId, pushHistory, reducedMotion);
  } catch (err) {
    if (currBody) {
      currBody.style.minBlockSize = "";
      currBody.removeAttribute("aria-busy");
    }
    if (err?.name !== "AbortError") location.href = url.href;
  }
}

function buildCleanGetUrl(form) {
  const url = new URL(form.getAttribute("action") || location.pathname, location.href);
  url.search = "";
  const data = new FormData(form);
  for (const [key, raw] of data.entries()) {
    const val = String(raw).trim();
    if (!val) continue;
    if (key === "sort" && val === "newest") continue;
    url.searchParams.set(key, val);
  }
  return url;
}

function applySnapshotDoc(parsed, url) {
  const nextMain = parsed.querySelector("main#main");
  const currMain = document.querySelector("main#main");
  if (!nextMain || !currMain) {
    location.href = url.href;
    return false;
  }

  const currShell = currMain.querySelector(".project-shell");
  const nextStage = nextMain.querySelector("[data-project-stage]");
  const nextShell = nextMain.querySelector(".project-shell");
  const sameProject = Boolean(currShell && nextStage && !nextShell);

  const nextHead = nextMain.querySelector(".project-masthead");
  document.title = nextHead?.dataset.pageTitle || parsed.title || document.title;

  const reducedMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  const isDesktop = window.matchMedia("(min-width: 49rem)").matches;
  const useViewTransition = !reducedMotion && typeof document.startViewTransition === "function";

  const commitDom = () => {
    if (sameProject) {
      const currHead = currMain.querySelector(".project-masthead");
      const currStage = currMain.querySelector("[data-project-stage]");
      if (currHead && nextHead) currHead.replaceWith(nextHead);
      if (currStage && nextStage) {
        if (!reducedMotion && !useViewTransition) {
          nextStage.classList.add("is-entering");
          nextStage.addEventListener("animationend", () => nextStage.classList.remove("is-entering"), { once: true });
        }
        currStage.replaceWith(nextStage);
      }
      hydrateMain(currMain);
    } else {
      const currRail = currMain.querySelector("[data-project-rail]");
      const nextRail = nextMain.querySelector("[data-project-rail]");
      if (currRail && !nextRail && !reducedMotion && !useViewTransition && isDesktop) {
        document.body.appendChild(currRail);
        currRail.classList.add("is-leaving");
        currRail.addEventListener("animationend", () => currRail.remove(), { once: true });
      }
      if (!currRail && nextRail && !reducedMotion && !useViewTransition) {
        nextRail.classList.add("is-entering");
        nextRail.addEventListener("animationend", () => nextRail.classList.remove("is-entering"), { once: true });
      }
      currMain.replaceWith(nextMain);
      hydrateMain(nextMain);
      positionRailIndicator(document.querySelector("[data-project-rail]"), true);
    }

    for (const a of document.querySelectorAll(".workspace-rail a[href]")) {
      const linkPath = new URL(a.href, location.href).pathname;
      if (url.pathname === linkPath || (linkPath !== "/dashboard" && url.pathname.startsWith(`${linkPath}/`))) {
        a.setAttribute("aria-current", "page");
      } else {
        a.removeAttribute("aria-current");
      }
    }
    window.scrollTo({ top: 0, behavior: "instant" });
  };

  if (useViewTransition) {
    document.documentElement.dataset.navDir = url.datasetNavDir || "down";
    const activeRail = document.querySelector("[data-project-rail]");
    if (activeRail) {
      activeRail.classList.add("is-navigating");
      void activeRail.offsetHeight;
    }
    const transition = document.startViewTransition(commitDom);
    return transition.finished
      .catch(() => {})
      .finally(() => {
        delete document.documentElement.dataset.navDir;
        activeRail?.classList.remove("is-navigating");
      });
  }
  commitDom();
  return Promise.resolve();
}

function applySnapshotHtml(html, url) {
  return applySnapshotDoc(new DOMParser().parseFromString(html, "text/html"), url);
}

async function navigatePartial(href, pushHistory = true, navDir = null) {
  swapCache.clear();
  const url = new URL(href, location.href);
  url.datasetNavDir = navDir || (pushHistory ? "down" : "up");
  if (url.origin !== location.origin) {
    location.href = href;
    return;
  }
  const rail = document.querySelector("[data-project-rail]");
  if (rail) {
    const targetLink = [...rail.querySelectorAll("a")].find((a) => new URL(a.href, location.href).pathname === url.pathname);
    if (targetLink) {
      for (const a of rail.querySelectorAll("a")) {
        if (a === targetLink) a.setAttribute("aria-current", "page");
        else a.removeAttribute("aria-current");
      }
      positionRailIndicator(rail, false);
    }
  }
  navAbort?.abort();
  navAbort = new AbortController();
  const currProjectId = document.querySelector("main#main .project-shell")?.dataset.projectId;
  const inSameProject = Boolean(
    currProjectId &&
    (url.pathname === `/projects/${currProjectId}` || url.pathname.startsWith(`/projects/${currProjectId}/`)),
  );
  const headers = { Accept: "application/x-ndjson, text/html;q=0.9" };
  if (inSameProject) headers["X-Rupicola-Partial"] = "1";
  try {
    const resp = await fetch(url.href, {
      headers,
      credentials: "same-origin",
      signal: navAbort.signal,
    });
    if (!resp.ok) {
      location.href = url.href;
      return;
    }
    const contentType = resp.headers.get("content-type") || "";
    if (contentType.includes("application/x-ndjson") && resp.body) {
      const reader = resp.body.getReader();
      const decoder = new TextDecoder();
      let buffer = "";
      let pushed = false;
      const pendingSwaps = [];
      let snapshotDone = Promise.resolve();
      let stagedDoc = null;
      let committed = false;
      let skeletonTimer = null;

      const commitStaged = () => {
        if (committed || !stagedDoc) return;
        committed = true;
        if (skeletonTimer) {
          clearTimeout(skeletonTimer);
          skeletonTimer = null;
        }
        snapshotDone = Promise.resolve(applySnapshotDoc(stagedDoc, url));
        if (pushHistory && url.href !== location.href && !pushed) {
          history.pushState({ partial: true }, "", url.href);
          pushed = true;
        }
      };

      const handleLine = async (line) => {
        if (!line.trim()) return;
        const frame = JSON.parse(line);
        if (frame.t === "snapshot") {
          stagedDoc = new DOMParser().parseFromString(frame.html, "text/html");
          // Wait briefly (55ms) so fast swap frames fold directly into the single View Transition
          // without flashing a skeleton and then blinking again on hydration.
          skeletonTimer = setTimeout(commitStaged, 55);
        } else if (frame.t === "swap") {
          if (!committed && stagedDoc) {
            applyTopcoatSwapIn(stagedDoc, frame.region, frame.html, false);
          } else if (!stagedDoc) {
            pendingSwaps.push(frame);
          } else {
            await snapshotDone;
            applyTopcoatSwap(frame.region, frame.html);
          }
        } else if (frame.t === "redirect") {
          if (skeletonTimer) clearTimeout(skeletonTimer);
          location.href = frame.location;
        }
      };

      while (true) {
        const { done, value } = await reader.read();
        if (done) break;
        buffer += decoder.decode(value, { stream: true });
        let newline = buffer.indexOf("\n");
        while (newline !== -1) {
          const line = buffer.slice(0, newline);
          buffer = buffer.slice(newline + 1);
          await handleLine(line);
          newline = buffer.indexOf("\n");
        }
      }
      buffer += decoder.decode();
      if (buffer.trim()) await handleLine(buffer);
      commitStaged();
      await snapshotDone;
      while (pendingSwaps.length > 0) {
        const swap = pendingSwaps.shift();
        applyTopcoatSwap(swap.region, swap.html);
      }
      return;
    }
    if (!contentType.includes("text/html")) {
      location.href = url.href;
      return;
    }
    await applySnapshotHtml(await resp.text(), url);
    if (pushHistory && url.href !== location.href) {
      history.pushState({ partial: true }, "", url.href);
    }
  } catch (err) {
    if (err?.name !== "AbortError") location.href = url.href;
  }
}

document.addEventListener("submit", (event) => {
  const form = event.target.closest("form");
  if (!form) return;
  if ((form.method || "get").toLowerCase() !== "get") {
    swapCache.clear();
    return;
  }
  if (!form.hasAttribute("data-swap-target")) return;
  event.preventDefault();
  const url = buildCleanGetUrl(form);
  swapRegion(url.href, form.dataset.swapTarget, true);
});

document.addEventListener("click", (event) => {
  if (event.defaultPrevented || event.button !== 0 || event.ctrlKey || event.metaKey || event.shiftKey || event.altKey) return;

  const trigger = event.target.closest(".ui-dropdown-trigger");
  if (trigger) {
    const dd = trigger.closest("[data-dropdown]");
    if (!dd) return;
    const opening = !dd.classList.contains("is-open");
    closeAllDropdowns(opening ? dd : null);
    setDropdownOpen(dd, opening);
    if (opening) {
      const menu = dd.querySelector(".ui-dropdown-menu");
      (menu?.querySelector('[aria-selected="true"]') || menu?.querySelector("[data-dropdown-option]"))?.focus();
    }
    return;
  }

  const optBtn = event.target.closest("[data-dropdown-option]");
  if (optBtn) {
    const dd = optBtn.closest("[data-dropdown]");
    if (dd) selectDropdownOption(dd, optBtn);
    return;
  }

  if (!event.target.closest("[data-dropdown]")) {
    closeAllDropdowns();
  }

  const swapLink = event.target.closest("a[data-swap-target]");
  if (swapLink && swapLink.target !== "_blank") {
    const url = new URL(swapLink.href, location.href);
    if (url.origin === location.origin) {
      event.preventDefault();
      const seg = swapLink.closest("[data-segmented]");
      if (seg) {
        for (const a of seg.querySelectorAll("a")) {
          if (a === swapLink) a.setAttribute("aria-current", "page");
          else a.removeAttribute("aria-current");
        }
        positionSegmentedIndicator(seg, false);
      }
      swapRegion(url.href, swapLink.dataset.swapTarget, true);
      return;
    }
  }

  const link = event.target.closest("[data-nav-partial], .workspace-rail a[href], .project-card[href]");
  if (!link || link.target === "_blank") return;
  const url = new URL(link.href, location.href);
  if (url.origin !== location.origin) return;
  event.preventDefault();
  if (link.closest("[data-project-rail], .workspace-rail")) {
    link.classList.add("is-tooltip-muted");
    const unmute = () => link.classList.remove("is-tooltip-muted");
    link.addEventListener("pointerleave", unmute, { once: true });
    link.addEventListener("blur", unmute, { once: true });
  }
  let navDir = "down";
  const railContainer = link.closest("[data-project-rail], .workspace-rail");
  const currentLink = railContainer?.querySelector('a[aria-current="page"]');
  if (currentLink && currentLink !== link) {
    const currRect = currentLink.getBoundingClientRect();
    const nextRect = link.getBoundingClientRect();
    const horizontal = railContainer.matches("[data-project-rail]") && window.matchMedia("(width < 49rem)").matches;
    if (horizontal ? nextRect.left < currRect.left - 2 : nextRect.top < currRect.top - 2) {
      navDir = "up";
    }
  }
  navigatePartial(url.href, true, navDir);
});

window.addEventListener("popstate", (event) => {
  const targetId = event.state?.swapTarget;
  if (targetId && document.getElementById(targetId)) {
    swapRegion(location.href, targetId, false);
    return;
  }
  navigatePartial(location.href, false);
});

const preferences = ["dark", "light", "system"];
const themeChoice = document.querySelector("[data-theme-choice]");
const themeToggleBtn = document.querySelector("[data-theme-toggle]");
const themeTooltip = document.querySelector("[data-theme-tooltip]");
const resolvedTheme = (mode) => {
  if (mode === "light" || mode === "dark") return mode;
  return window.matchMedia?.("(prefers-color-scheme: light)").matches ? "light" : "dark";
};
const syncThemeToggle = (mode) => {
  if (!themeToggleBtn) return;
  const active = resolvedTheme(mode);
  const nextLabel = active === "light" ? "Switch to dark theme" : "Switch to light theme";
  const tipLabel = active === "light" ? "Dark theme" : "Light theme";
  themeToggleBtn.setAttribute("aria-label", nextLabel);
  themeToggleBtn.setAttribute("title", nextLabel);
  themeToggleBtn.dataset.themeState = active;
  if (themeTooltip) themeTooltip.textContent = tipLabel;
};
try {
  const initial = document.documentElement.dataset.theme || localStorage.getItem("rupicola-theme") || "dark";
  if (preferences.includes(initial)) {
    document.documentElement.dataset.theme = initial;
    if (themeChoice) {
      themeChoice.value = initial;
      syncDropdownValue(themeChoice, initial);
    }
    syncThemeToggle(initial);
  }
  new MutationObserver(() => {
    syncThemeToggle(document.documentElement.dataset.theme || "dark");
  }).observe(document.documentElement, { attributes: true, attributeFilter: ["data-theme"] });
  const applyTheme = (mode) => {
    const root = document.documentElement;
    root.classList.add("theme-switching");
    root.dataset.theme = mode;
    localStorage.setItem("rupicola-theme", mode);
    if (themeChoice) {
      themeChoice.value = mode;
      syncDropdownValue(themeChoice, mode);
    }
    syncThemeToggle(mode);
    void root.offsetHeight;
    requestAnimationFrame(() => root.classList.remove("theme-switching"));
  };
  themeChoice?.addEventListener("change", () => {
    if (!preferences.includes(themeChoice.value)) return;
    applyTheme(themeChoice.value);
  });
  themeToggleBtn?.addEventListener("click", () => {
    const curr = resolvedTheme(document.documentElement.dataset.theme || "dark");
    applyTheme(curr === "light" ? "dark" : "light");
  });
} catch {
  if (themeChoice) themeChoice.disabled = true;
}

document.addEventListener("change", (event) => {
  const select = event.target.closest("[data-auto-submit]");
  if (select && select.form) select.form.requestSubmit();
});

document.addEventListener("click", (event) => {
  const opener = event.target.closest("[data-open-target]");
  if (opener) {
    const target = document.getElementById(opener.dataset.openTarget);
    if (target) {
      if (target.tagName === "DETAILS") target.open = true;
      target.scrollIntoView({ behavior: "smooth", block: "nearest" });
      target.querySelector("input, textarea, select")?.focus();
    }
    return;
  }
  for (const menu of document.querySelectorAll(".account-menu[open]")) {
    if (!menu.contains(event.target)) menu.removeAttribute("open");
  }
});
document.addEventListener("keydown", (event) => {
  const openDd = document.querySelector("[data-dropdown].is-open");
  if (openDd) {
    const options = [...openDd.querySelectorAll("[data-dropdown-option]")];
    const activeIdx = options.indexOf(document.activeElement);
    if (event.key === "Escape") {
      event.preventDefault();
      closeAllDropdowns();
      openDd.querySelector(".ui-dropdown-trigger")?.focus();
      return;
    }
    if (event.key === "ArrowDown") {
      event.preventDefault();
      options[(activeIdx + 1) % options.length]?.focus();
      return;
    }
    if (event.key === "ArrowUp") {
      event.preventDefault();
      options[(activeIdx - 1 + options.length) % options.length]?.focus();
      return;
    }
    if (event.key === "Home") {
      event.preventDefault();
      options[0]?.focus();
      return;
    }
    if (event.key === "End") {
      event.preventDefault();
      options[options.length - 1]?.focus();
      return;
    }
  }
  if (event.key !== "Escape") return;
  for (const menu of document.querySelectorAll(".account-menu[open]")) {
    menu.removeAttribute("open");
    menu.querySelector("summary")?.focus();
  }
});
