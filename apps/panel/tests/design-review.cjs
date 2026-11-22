// Run against an isolated, fresh panel with its Chromium validator configured.
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const { chromium } = require(process.env.RUPICOLA_PLAYWRIGHT_MODULE || "playwright");
const testURL = new URL(process.env.RUPICOLA_TEST_ORIGIN || "http://127.0.0.1:4731");
assert(["localhost", "127.0.0.1"].includes(testURL.hostname), "use a fresh loopback panel");
const origin = testURL.origin;
const artifacts = process.env.RUPICOLA_TEST_ARTIFACTS;

async function run() {
  const browser = await chromium.launch();
  try {
    const page = await browser.newPage();
    await page.goto(origin + "/register");
    await page.locator("#handle").fill("design-review-test");
    await page.locator("#password").fill("isolated-test-password");
    await page.locator('form button[type="submit"]').click();
    await page.waitForURL("**/dashboard");

    const post = (path, form) => page.request.post(origin + path, {
      headers: { Origin: origin }, form, maxRedirects: 0, timeout: 65_000,
    });
    async function redirect(response) {
      assert.equal(response.status(), 303, await response.text());
      return response.headers().location;
    }
    async function fields(path) {
      await page.goto(origin + path);
      const data = {};
      for (const name of ["csrf", "version", "generation"]) {
        const field = page.locator('input[name="' + name + '"]').first();
        if (await field.count()) data[name] = await field.inputValue();
      }
      return data;
    }

    const project = "design-review";
    const base = "/projects/" + project + "/design";
    const initial = await fields("/projects");
    await redirect(await post("/projects", { ...initial, id: project, label: "Design review" }));
    const legacy = await page.request.get(origin + "/projects/" + project + "/proposes?kind=page", { maxRedirects: 0 });
    assert.equal(await redirect(legacy), base + "?view=proposals&kind=page");

    const created = await redirect(await post(base + "/version", {
      ...await fields(base + "?view=system"), action: "create",
    }));
    const id = new URL(created, origin).searchParams.get("version");
    const screens = base + "?view=screens&version=" + id;
    const proposals = base + "?view=proposals&version=" + id;
    const action = async (name, path, data = {}) =>
      post(base + "/version", { ...await fields(path), action: name, ...data });

    assert.equal((await action("submit", screens)).status(), 409);
    const change = async (accent, expected, currentId = id) => {
      const fieldsBefore = await fields(base + "?view=system&version=" + currentId);
      const document = {
        ...fieldsBefore, name: "Rupicola Design System", brand: "Rupicola",
        landing_title: "Interfaces under observation.", queue_title: "Review captures",
        queue_item: "Compact notification settings", settings_title: "Workspace settings",
        direction: "Warm paper and orange.",
        usage: "Orange for the primary action.", screen_title: "Custom comparison",
        scenario: "Settings example", canvas: "#f7f1e7", surface: "#fffaf2",
        ink: "#231d19", accent, action_ink: "#231d19", success: "#40543c", error: "#9c3617",
        spacing: 16, radius: 6, heading: "editorial", before_label: "Earlier", after_label: "Candidate",
        button_notes: "Use one primary action.", form_notes: "Label each control.",
        table_notes: "Use row and column headings.", comparison_notes: "Keep sample data aligned.",
      };
      await redirect(await post(base + "/edit", document));
      assert.equal((await post(base + "/edit", document)).status(), 409);
      const report = await redirect(await action("validate", base + "?view=screens&version=" + currentId));
      await page.goto(origin + report);
      assert(await page.getByText(expected, { exact: true }).isVisible(), await page.locator("main").innerText());
    };
    await change("#231d19", "Validation failed.");
    assert.equal((await action("submit", screens)).status(), 409);
    await change("#ff681e", "Required checks passed.");

    const frame = page.frameLocator('iframe[title="Design system examples"]');
    await frame.getByRole("heading", { name: "Component library" }).waitFor();
    await frame.getByRole("link", { name: "Evidence review", exact: true }).click();
    await frame.getByRole("heading", { name: "Custom comparison" }).waitFor();
    await frame.getByRole("button", { name: "Clips" }).click();
    assert.equal(await frame.locator("video:visible").count(), 2);
    await frame.locator("#after-clip video").evaluate(video => video.play());
    const proof = await page.request.get(origin + base + "/proof/" + id + "/320-clips");
    assert.equal(proof.headers()["content-type"], "image/png");
    const anonymous = await browser.newContext();
    const privateProof = await anonymous.request.get(origin + base + "/proof/" + id + "/320-clips", { maxRedirects: 0 });
    assert.equal(privateProof.status(), 303);
    await anonymous.close();

    assert.equal((await action("submit", screens, { csrf: "wrong" })).status(), 403);
    await redirect(await action("submit", screens));
    await redirect(await action("comment", proposals, { target: "comparison", note: "Review both clips." }));
    await redirect(await action("approve", proposals));
    await redirect(await action("adopt", proposals));
    await page.goto(origin + proposals);
    assert(await page.getByText("Review both clips.", { exact: true }).isVisible());
    assert(await page.getByText("Adopted", { exact: true }).first().isVisible());
    const variant = await redirect(await action("variation", proposals));
    const parent = new URL(variant, origin).searchParams.get("version");
    assert.notEqual(parent, id);
    await page.goto(origin + base + "?view=proposals&version=" + parent);
    assert((await page.getByText("Parent version").getAttribute("href")).endsWith(id));
    assert(await page.getByText("Validate before sending for review").isVisible());
    await change("#f2b348", "Required checks passed.", parent);
    assert(await page.getByRole("link", { name: "1440-baseline-images", exact: true }).isVisible());
    const screenshot = async (name, versionId = parent) => {
      const response = await page.request.get(origin + base + "/proof/" + versionId + "/" + name);
      assert.equal(response.status(), 200);
      return response.body();
    };
    const candidateImage = await screenshot("1440-images");
    const baselineImage = await screenshot("1440-baseline-images");
    assert.notDeepEqual(candidateImage, baselineImage);
    if (artifacts) {
      fs.mkdirSync(artifacts, { recursive: true });
      fs.writeFileSync(path.join(artifacts, "candidate.png"), candidateImage);
      fs.writeFileSync(path.join(artifacts, "baseline.png"), baselineImage);
      fs.writeFileSync(path.join(artifacts, "mobile-clips.png"), await screenshot("320-clips"));
      for (const scene of ["components", "home", "queue", "settings"]) {
        for (const width of [320, 1440]) {
          fs.writeFileSync(path.join(artifacts, scene + "-" + width + ".png"), await screenshot(width + "-" + scene, id));
        }
      }
      const html = await page.request.get(origin + base + "/preview/" + id);
      assert.equal(html.status(), 200);
      fs.writeFileSync(path.join(artifacts, "comparacao-interativa.html"), await html.body());
      await page.goto(origin + base + "?view=system&version=" + parent);
      await page.screenshot({ path: path.join(artifacts, "system.png"), fullPage: true });
    }
    console.log("Design review passed: failed contrast, stale write, browser checks, proof access, review, adoption and variation.");
  } finally {
    await browser.close();
  }
}

run().catch(error => { console.error(error); process.exitCode = 1; });
