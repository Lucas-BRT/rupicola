const fs = require("node:fs");
const assert = require("node:assert/strict");
const deadline = setTimeout(() => process.exit(124), 55_000);
const checks = [];
const shots = [];

async function openTemplate(browser, width, html) {
  const page = await browser.newPage({ viewport: { width, height: 900 } });
  page.setDefaultTimeout(3500);
  // The preview needs no origin, authentication state, or network access.
  await page.route("**/*", route => route.abort());
  const errors = [];
  page.on("pageerror", error => errors.push(error.message));
  await page.setContent(html, { waitUntil: "load" });
  return { page, errors };
}

async function capture(page, width, state) {
  const bytes = await page.screenshot({ animations: "disabled", fullPage: true });
  assert(bytes.length <= 512 * 1024, "capture exceeds limit");
  shots.push({ name: width + "-" + state, png: bytes.toString("base64") });
}

const clipSelectors = ["#before-clip video", "#after-clip video"];

async function seekSample(video) {
  await video.evaluate(node => {
    node.pause();
    if (Math.abs(node.currentTime - 0.75) < 0.005) return;
    return new Promise(resolve => {
      node.addEventListener("seeked", resolve, { once: true });
      node.currentTime = 0.75;
    });
  });
}

async function example(page, id) {
  const link = page.locator('.preview-nav a[href="#' + id + '"]');
  await link.click();
  await page.locator("#" + id).waitFor({ state: "visible" });
  assert.equal(await link.getAttribute("aria-current"), "page");
  assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), id + " overflows");
}

async function focusComparison(page) {
  await example(page, "evidence");
  await page.locator("#side").focus();
  await page.keyboard.press("Tab");
  assert(await page.locator("#overlay").evaluate(node => node === document.activeElement && node.matches(":focus-visible")), "keyboard focus is missing");
}

async function checkExamples(page, width) {
  await example(page, "components");
  await capture(page, width, "components");
  assert.equal(await page.locator("#components section").count(), 6);
  assert(await page.getByRole("button", { name: "Unavailable action" }).isDisabled());
  await page.locator("#sample-primary").click();
  assert(await page.locator("#button-feedback").isVisible());
  await page.locator("#sample-secondary").click();
  assert(await page.locator("#button-feedback").isHidden());
  checks.push(width + " px: six component groups, disabled action and feedback");

  await example(page, "home");
  assert(await page.locator(".landing-media img").evaluate(img => img.naturalWidth > 0));
  await capture(page, width, "home");
  await page.getByRole("link", { name: "Open work queue" }).click();
  await page.locator("#queue").waitFor({ state: "visible" });
  checks.push(width + " px: home renders evidence and opens the work queue");

  await capture(page, width, "queue");
  await page.locator("#queue-status").selectOption("reviewed");
  assert.equal(await page.locator("#queue tbody tr:visible").count(), 1);
  await page.locator("#queue-search").fill("!".repeat(150));
  assert(await page.locator("#queue-empty").isVisible());
  await page.locator("#clear-queue").click();
  assert.equal(await page.locator("#queue tbody tr:visible").count(), 3);
  assert(await page.locator("#queue-search").evaluate(node => node === document.activeElement));
  assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), "queue overflows");
  if (width === 320) {
    assert(await page.locator("#queue-table").evaluate(node => node.scrollWidth === node.clientWidth), "work queue actions overflow the mobile table");
  }
  await page.locator("#queue-table").getByRole("link", { name: "Review", exact: true }).first().click();
  await page.locator("#evidence").waitFor({ state: "visible" });
  checks.push(width + " px: queue filters, empty state, reset and open review");

  await example(page, "settings");
  await capture(page, width, "settings");
  await page.locator("#digest-enabled").check();
  await page.getByRole("button", { name: "Save settings" }).click();
  assert(await page.locator("#digest-email").evaluate(node => !node.validity.valid && document.activeElement === node));
  await page.locator("#digest-email").fill("review@example.test");
  await page.locator("#workspace-name").fill("Example workspace");
  await page.locator("#evidence-density").selectOption("compact");
  await page.getByRole("button", { name: "Save settings" }).click();
  assert(await page.locator("#settings-result").isVisible());
  assert.equal(await page.locator("#preview-brand").textContent(), "Example workspace");
  assert.equal(await page.locator("body").getAttribute("data-density"), "compact");
  await page.getByRole("button", { name: "Reset fields" }).click();
  await page.locator("#digest-fields").waitFor({ state: "hidden" });
  assert(await page.locator("#settings-result").isHidden());
  checks.push(width + " px: settings validation, save and reset");
}

async function captureBaseline(browser, width, html) {
  const { page, errors } = await openTemplate(browser, width, html);
  await focusComparison(page);
  await capture(page, width, "baseline-images");
  await page.locator("#clips").click();
  for (const id of clipSelectors) {
    const video = page.locator(id);
    await video.evaluate(node => node.play());
    await seekSample(video);
  }
  await capture(page, width, "baseline-clips");
  for (const id of ["components", "home", "queue", "settings"]) {
    await example(page, id);
    await capture(page, width, "baseline-" + id);
  }
  assert.equal(errors.length, 0, errors.join(" / "));
  await page.close();
  checks.push(width + " px: adopted baseline with matching example data");
}

async function run() {
  let browser;
  let version = null;
  try {
    const { html, baseline } = JSON.parse(fs.readFileSync(0, "utf8"));
    const { chromium } = require(process.env.RUPICOLA_PLAYWRIGHT_MODULE || "playwright");
    browser = await chromium.launch({
      headless: true,
      executablePath: process.env.RUPICOLA_CHROMIUM || undefined,
      timeout: 10_000,
    });
    version = browser.version();
    for (const width of [320, 1440]) {
      const { page, errors } = await openTemplate(browser, width, html);
      await focusComparison(page);
      assert(await page.locator("#evidence h1").isVisible(), "title missing");
      assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), "page overflows");
      assert(await page.locator("#stage img").evaluateAll(images => images.every(img => img.naturalWidth > 0)), "image fails to decode");
      await capture(page, width, "images");
      checks.push(width + " px: image decode, layout and keyboard focus");

      await page.locator("#overlay").click();
      await page.locator("#split").focus();
      await page.keyboard.press("ArrowRight");
      assert.equal(await page.locator("#split").inputValue(), "51");
      assert.equal(await page.locator("#stage").evaluate(node => node.style.getPropertyValue("--split")), "51%");
      assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), "overlay overflows");
      assert(await page.locator("#after img").evaluate(node => getComputedStyle(node).clipPath !== "none"));
      await page.locator("#before img").evaluate(img => { img.src = "data:image/png;base64,AA"; });
      await page.getByRole("alert").waitFor({ state: "visible" });
      await page.locator("#retry").click();
      assert(await page.locator("#stage").isVisible(), "retry did not restore evidence");
      await page.waitForFunction(() => document.querySelector("#before img").naturalWidth > 0);
      assert(await page.locator("#side").evaluate(node => node === document.activeElement), "retry did not restore focus");
      checks.push(width + " px: overlay slider and unavailable-media recovery");

      await page.locator("#clips").click();
      for (const id of clipSelectors) {
        const video = page.locator(id);
        assert(await video.isVisible(), "clip player is missing");
        await video.evaluate(node => node.play());
        await page.waitForFunction(selector => document.querySelector(selector).currentTime > 0.15, id, { timeout: 4000 });
        await seekSample(video);
      }
      await capture(page, width, "clips");
      checks.push(width + " px: both WebM clips decode and play");

      await page.locator("#record").click();
      assert(await page.locator("#decision").evaluate(node => !node.validity.valid && node === document.activeElement), "form accepted an empty decision");
      await page.locator("#decision").selectOption("changes");
      await page.locator("#note").fill("Review the shorter label.");
      await page.locator("#record").click();
      assert(await page.getByRole("status").isVisible(), "review feedback is missing");
      assert.equal(await page.locator("#example-status").textContent(), "Changes requested");
      assert(await page.locator("#record").isDisabled(), "duplicate submission is available");
      await page.locator("#clear").click();
      assert.equal(await page.locator("#example-status").textContent(), "Pending");
      assert(await page.locator("#record").isEnabled(), "reset did not re-enable the form");
      checks.push(width + " px: form validation, review and reset");
      await checkExamples(page, width);
      assert.equal(errors.length, 0, errors.join(" / "));
      if (baseline) await captureBaseline(browser, width, baseline);
      await page.close();
    }
    return { outcome: "passed", browser: version, checks, shots, error: null };
  } catch (error) {
    return { outcome: version ? "failed" : "blocked", browser: version, checks, shots: [], error: String(error.message).slice(0, 500) };
  } finally {
    await browser?.close();
  }
}

run().then(report => {
  clearTimeout(deadline);
  process.stdout.write(JSON.stringify(report));
});
