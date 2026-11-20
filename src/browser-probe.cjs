const fs = require("node:fs");
const path = require("node:path");
const { performance } = require("node:perf_hooks");

const req = JSON.parse(fs.readFileSync(0, "utf8"));
const {
  origin,
  pagesMode,
  pages,
  maxPages = 20,
  viewports,
  themes,
  depth,
  output,
  modulePath,
  timeoutMs = 10000,
} = req;

const out = {
  browser: null,
  blocker: null,
  discoveredPages: [],
  checkpoints: [],
  clips: [],
  findings: [],
  trees: [],
};

function slug(s) {
  return (
    s
      .replace(/^\/+/, "")
      .replace(/[^a-zA-Z0-9]+/g, "-")
      .replace(/^-+|-+$/g, "")
      .slice(0, 36) || "root"
  );
}

function loadPlaywright() {
  const candidates = [
    modulePath,
    "playwright",
    "/home/lucasbrt/Projects/pokemon-survivors/node_modules/playwright",
  ].filter(Boolean);
  for (const c of candidates) {
    try {
      return require(c);
    } catch {}
  }
  throw new Error("Playwright is unavailable. Install it or pass --playwright-module.");
}

async function run() {
  let browser;
  try {
    const { chromium } = loadPlaywright();
    browser = await chromium.launch({ headless: true, timeout: timeoutMs });
    out.browser = await browser.version();

    // 1. Discover or normalize pages (with automatic setup/auth if landing on /register or /login)
    let authState = null;
    {
      const bootCtx = await browser.newContext({ viewport: { width: 1440, height: 900 } });
      const bootPage = await bootCtx.newPage();
      try {
        await bootPage.goto(origin + "/register", { waitUntil: "domcontentloaded", timeout: timeoutMs });
        const canSetup = await bootPage.evaluate(
          () => !!document.querySelector("#handle") && !!document.querySelector("#password") && !document.querySelector("#code")
        );
        if (canSetup) {
          await bootPage.fill("#handle", process.env.FERRIS_PROBE_USER || "probe-owner");
          await bootPage.fill("#password", process.env.FERRIS_PROBE_PASS || "ProbeMatrix-2026!");
          await bootPage.locator('button[type="submit"]').click();
          await bootPage.waitForURL((u) => !u.pathname.startsWith("/register"), { timeout: timeoutMs });
          authState = await bootCtx.storageState();
        } else {
          await bootPage.goto(origin + "/login", { waitUntil: "domcontentloaded", timeout: timeoutMs });
          if (await bootPage.evaluate(() => !!document.querySelector("#handle") && !!document.querySelector("#password"))) {
            await bootPage.fill("#handle", process.env.FERRIS_PROBE_USER || "probe-owner");
            await bootPage.fill("#password", process.env.FERRIS_PROBE_PASS || "ProbeMatrix-2026!");
            await bootPage.locator('button[type="submit"]').click();
            await bootPage.waitForURL((u) => !u.pathname.startsWith("/login"), { timeout: timeoutMs });
            authState = await bootCtx.storageState();
          }
        }
      } catch {}
      await bootCtx.close();
    }

    let targetPages = [...pages];
    if (pagesMode === "all") {
      const ctx = await browser.newContext({
        viewport: { width: 1440, height: 900 },
        ...(authState ? { storageState: authState } : {}),
      });
      const page = await ctx.newPage();
      const seen = new Set();
      const queue = targetPages.length ? [...targetPages] : ["/"];
      while (queue.length && seen.size < maxPages) {
        const route = queue.shift();
        if (seen.has(route)) continue;
        seen.add(route);
        try {
          const res = await page.goto(origin + route, { waitUntil: "domcontentloaded", timeout: timeoutMs });
          if (!res || res.status() >= 400) continue;
          const links = await page.evaluate((orig) => {
            return [...document.querySelectorAll("a[href]")]
              .map((a) => {
                try {
                  const u = new URL(a.getAttribute("href"), orig);
                  return u.origin === orig ? u.pathname : null;
                } catch {
                  return null;
                }
              })
              .filter(Boolean);
          }, origin);
          for (const l of links) {
            const isRun = l.startsWith("/run/");
            const hasRun = [...seen, ...queue].some((r) => r.startsWith("/run/"));
            if (isRun && hasRun) continue;
            if (
              !seen.has(l) &&
              !queue.includes(l) &&
              !l.startsWith("/logout") &&
              !l.startsWith("/media/") &&
              !l.endsWith(".css") &&
              !l.endsWith(".js")
            ) {
              queue.push(l);
            }
          }
        } catch {}
      }
      await ctx.close();
      targetPages = [...seen];
    }
    out.discoveredPages = targetPages;
    fs.mkdirSync(path.join(output, "checkpoints"), { recursive: true });

    // 2. Matrix: theme x viewport x route
    for (const theme of themes) {
      for (const vp of viewports) {
        const ctx = await browser.newContext({
          viewport: { width: vp.width, height: vp.height },
          colorScheme: theme === "light" ? "light" : "dark",
          ...(authState ? { storageState: authState } : {}),
        });
        await ctx.addInitScript((t) => {
          try {
            localStorage.setItem("ferris-theme", t);
            document.documentElement.dataset.theme = t;
          } catch {}
        }, theme);

        const page = await ctx.newPage();
        for (const route of targetPages) {
          const consoleErrors = [];
          const onConsole = (m) => {
            if (m.type() === "error") consoleErrors.push(m.text());
          };
          const onPageError = (e) => consoleErrors.push(e.message);
          page.on("console", onConsole);
          page.on("pageerror", onPageError);

          const cellId = `${slug(route)}-${vp.width}x${vp.height}-${theme}`.slice(0, 60);
          const shotRel = path.join("checkpoints", `${cellId}.png`);
          try {
            const res = await page.goto(origin + route, { waitUntil: "load", timeout: timeoutMs });
            await page.evaluate(async (t) => {
              document.documentElement.dataset.theme = t;
              const ih = window.innerHeight;
              const visible = [...document.querySelectorAll("img")].filter((img) => {
                if (img.closest("dialog:not([open])")) return false;
                const r = img.getBoundingClientRect();
                return r.top < ih && r.bottom > 0;
              });
              await Promise.all(
                visible.map((img) => {
                  if (img.complete) return Promise.resolve();
                  return new Promise((res) => {
                    img.addEventListener("load", res, { once: true });
                    img.addEventListener("error", res, { once: true });
                    setTimeout(res, 1500);
                  });
                })
              );
            }, theme);
            await page.screenshot({ path: path.join(output, shotRel), timeout: timeoutMs });
            out.checkpoints.push({ id: cellId, path: shotRel });

            if (res && res.status() >= 400) {
              out.findings.push({
                note: `measured · [${route} @ ${vp.width}×${vp.height} ${theme}] HTTP ${res.status()}`,
                evidence: shotRel,
              });
            }

            // D1 DOM + layout + tree extraction
            const d1 = await page.evaluate(() => {
              const iw = window.innerWidth;
              const ih = window.innerHeight;
              const sh = Math.max(ih, document.documentElement.scrollHeight);
              const sw = document.documentElement.scrollWidth;
              const overflowX = sw > iw + 1;
              const brokenImgs = [...document.querySelectorAll("img")]
                .filter((img) => {
                  if (img.closest("dialog:not([open]), [hidden]")) return false;
                  const r = img.getBoundingClientRect();
                  if (img.loading === "lazy" && r.top >= ih) return false;
                  return !img.complete || img.naturalWidth === 0;
                })
                .map((img) => img.getAttribute("src") || "<empty>");
              const h1Count = document.querySelectorAll("h1").length;
              const nodes = [];
              const els = [...document.querySelectorAll("a, button, input, select, textarea, summary, h1, h2")];
              let idx = 0;
              for (const el of els) {
                if (el.closest("dialog:not([open])") || el.classList.contains("skip")) continue;
                const closedDetails = el.closest("details:not([open])");
                if (closedDetails && el.tagName !== "SUMMARY") continue;
                if (el.matches("a") && el.querySelector("img, .studio-proof-image")) continue;
                const r = el.getBoundingClientRect();
                if (r.width < 1 || r.height < 1 || r.bottom <= 0 || r.bottom > ih || r.left < 0 || r.right > iw + 2) continue;
                const scrollParent = el.closest(".tabs");
                if (scrollParent) {
                  const pr = scrollParent.getBoundingClientRect();
                  if (r.left < pr.left - 1 || r.right > pr.right + 1) continue;
                }
                const cs = getComputedStyle(el);
                if (cs.visibility === "hidden" || cs.display === "none" || Number(cs.opacity) === 0) continue;
                const x = Math.round(r.left);
                const y = Math.round(r.top);
                const w = Math.round(r.width);
                const h = Math.round(r.height);
                const interactive = el.matches("a, button, input, select, textarea, summary");
                const name = (
                  el.getAttribute("aria-label") ||
                  el.innerText ||
                  el.getAttribute("placeholder") ||
                  el.getAttribute("title") ||
                  el.getAttribute("name") ||
                  "control"
                )
                  .trim()
                  .slice(0, 60) || "control";
                nodes.push({
                  id: `n${idx++}`,
                  role: el.tagName.toLowerCase(),
                  name,
                  x,
                  y,
                  w,
                  h,
                  interactive,
                  clipped: el.scrollWidth > el.clientWidth + 4 && cs.overflowX === "hidden",
                });
              }
              return { overflowX, sw, iw, ih: sh, brokenImgs, h1Count, nodes };
            });

            if (d1.overflowX) {
              out.findings.push({
                note: `measured · [${route} @ ${vp.width}×${vp.height} ${theme}] horizontal overflow (${d1.sw}px > ${d1.iw}px)`,
                evidence: shotRel,
              });
            }
            if (d1.brokenImgs.length) {
              out.findings.push({
                note: `measured · [${route} @ ${vp.width}×${vp.height} ${theme}] broken images: ${d1.brokenImgs.join(", ")}`,
                evidence: shotRel,
              });
            }
            if (d1.h1Count === 0) {
              out.findings.push({
                note: `measured · [${route} @ ${vp.width}×${vp.height} ${theme}] missing <h1> heading`,
                evidence: shotRel,
              });
            }
            if (consoleErrors.length) {
              out.findings.push({
                note: `measured · [${route} @ ${vp.width}×${vp.height} ${theme}] console error: ${consoleErrors[0].slice(0, 120)}`,
                evidence: shotRel,
              });
            }
            out.trees.push({
              cellId,
              route,
              viewport: `${vp.width}x${vp.height}`,
              theme,
              evidence: shotRel,
              tree: { window: { w: d1.iw, h: d1.ih }, nodes: d1.nodes },
            });

            // D3 Full Interaction & Motion Map
            if (depth === "d3") {
              // 1. Hover layout-shift & focus probe on deduplicated interactive selectors
              const targets = await page.evaluate(() => {
                const seenSig = new Set();
                const list = [];
                const controls = [...document.querySelectorAll("button:not(:disabled), a[href], summary, [role='tab']")];
                for (let i = 0; i < controls.length; i++) {
                  const el = controls[i];
                  if (el.closest("dialog:not([open])") || el.classList.contains("skip")) continue;
                  const r = el.getBoundingClientRect();
                  if (r.width < 4 || r.height < 4 || r.top < 0 || r.bottom > window.innerHeight) continue;
                  const sig = `${el.tagName}.${[...el.classList].sort().join(".")}`;
                  if (seenSig.has(sig) && list.length >= 6) continue;
                  seenSig.add(sig);
                  el.setAttribute("data-probe-idx", String(i));
                  list.push({
                    idx: i,
                    sig,
                    label: (el.innerText || el.getAttribute("aria-label") || sig).trim().slice(0, 32),
                    isTrigger:
                      el.tagName === "SUMMARY" ||
                      el.hasAttribute("data-viewer") ||
                      el.hasAttribute("data-proposal-open") ||
                      el.getAttribute("role") === "tab",
                  });
                  if (list.length >= 10) break;
                }
                return list;
              });

              for (const t of targets) {
                const loc = page.locator(`[data-probe-idx="${t.idx}"]`).first();
                try {
                  const beforeRects = await page.evaluate(() =>
                    [...document.querySelectorAll("h1, h2, button, a")].slice(0, 15).map((e) => {
                      const r = e.getBoundingClientRect();
                      return { top: r.top + window.scrollY, left: r.left + window.scrollX };
                    })
                  );
                  await loc.hover({ timeout: 1500 });
                  const afterRects = await page.evaluate(() =>
                    [...document.querySelectorAll("h1, h2, button, a")].slice(0, 15).map((e) => {
                      const r = e.getBoundingClientRect();
                      return { top: r.top + window.scrollY, left: r.left + window.scrollX };
                    })
                  );
                  const shifted = beforeRects.some(
                    (b, idx) =>
                      afterRects[idx] &&
                      (Math.abs(b.top - afterRects[idx].top) > 1.5 || Math.abs(b.left - afterRects[idx].left) > 1.5)
                  );
                  if (shifted) {
                    out.findings.push({
                      note: `measured · [${route} @ ${vp.width}×${vp.height}] hover layout shift triggered by "${t.label}"`,
                      evidence: shotRel,
                    });
                  }
                  await loc.focus({ timeout: 1500 });
                  const focusOk = await page.evaluate(() => {
                    const a = document.activeElement;
                    return a && a !== document.body && !a.disabled;
                  });
                  if (!focusOk) {
                    out.findings.push({
                      note: `measured · [${route} @ ${vp.width}×${vp.height}] focus lost or landed on disabled control for "${t.label}"`,
                      evidence: shotRel,
                    });
                  }

                  // Record motion clip on triggers (summary, modal opener, tab)
                  if (t.isTrigger && out.clips.length < 12) {
                    const clipId = `clip-${cellId}-${t.idx}`.replace(/[^a-zA-Z0-9_-]/g, "-").slice(0, 60);
                    const clipDir = path.join(output, "samples", clipId);
                    fs.mkdirSync(clipDir, { recursive: true });
                    const frames = [];
                    const f0 = path.join("samples", clipId, "000.png");
                    await page.screenshot({ path: path.join(output, f0), animations: "allow" });
                    frames.push({ path: f0, target_ms: 0, elapsed_ms: 0 });
                    const t0 = performance.now();
                    await loc.click({ timeout: 2000 });
                    for (let s = 1; s <= 4; s++) {
                      const target_ms = (s - 1) * 45;
                      const rem = target_ms - (performance.now() - t0);
                      if (rem > 0) await new Promise((r) => setTimeout(r, rem));
                      const fn = path.join("samples", clipId, String(s).padStart(3, "0") + ".png");
                      const elapsed_ms = Math.round(performance.now() - t0);
                      await page.screenshot({ path: path.join(output, fn), animations: "allow" });
                      frames.push({ path: fn, target_ms, elapsed_ms });
                    }
                    out.clips.push({ id: clipId, frames });

                    // If dialog opened, test Escape & focus return
                    const dialogOpen = await page.evaluate(() => !!document.querySelector("dialog[open]"));
                    if (dialogOpen) {
                      await page.keyboard.press("Escape");
                      const returned = await page.evaluate(
                        (idx) => document.activeElement?.getAttribute("data-probe-idx") === String(idx),
                        t.idx
                      );
                      if (!returned) {
                        out.findings.push({
                          note: `measured · [${route} @ ${vp.width}×${vp.height}] dialog Escape did not restore focus to opener "${t.label}"`,
                          evidence: shotRel,
                        });
                      }
                    }
                  }
                } catch {}
              }
            }
          } catch (err) {
            out.findings.push({
              note: `measured · [${route} @ ${vp.width}×${vp.height} ${theme}] probe error: ${String(err.message).split("\n")[0]}`,
              evidence: null,
            });
          } finally {
            page.off("console", onConsole);
            page.off("pageerror", onPageError);
          }
        }
        await ctx.close();
      }
    }
  } catch (err) {
    out.blocker = String(err.message).split("\n")[0];
  } finally {
    await browser?.close();
  }
}

run().finally(() => {
  process.stdout.write(JSON.stringify(out));
});
