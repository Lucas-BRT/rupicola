const fs = require("node:fs");
const path = require("node:path");
const { performance } = require("node:perf_hooks");

const request = JSON.parse(fs.readFileSync(0, "utf8"));
const { scenario, output, modulePath } = request;
const result = { browser: null, blocker: null, checkpoints: [], clips: [] };
const timeout = scenario.timeout_ms ?? 10000;

async function action(page, step) {
    const target = page.locator(step.selector);
    switch (step.kind) {
        case "click": return target.click();
        case "hover": return target.hover();
        case "fill": return target.fill(step.value);
        case "press": return target.press(step.key);
        case "ready": return target.waitFor({ state: "visible" });
    }
    throw new Error("unknown interaction");
}

async function record(page, step) {
    const directory = path.join(output, "samples", step.id);
    fs.mkdirSync(directory, { recursive: true });
    const clip = { id: step.id, frames: [] };
    result.clips.push(clip);
    const screenshot = (file) => page.screenshot({
        path: file, animations: "allow", timeout,
    });
    const baseline = path.join("samples", step.id, "000.png");
    await screenshot(path.join(output, baseline));
    clip.frames.push({ path: baseline, target_ms: 0, elapsed_ms: 0 });
    const started = performance.now();
    await action(page, step.trigger);
    for (let index = 1; index <= step.samples; index += 1) {
        const target_ms = (index - 1) * step.interval_ms;
        const remaining = target_ms - (performance.now() - started);
        if (remaining > 0) {
            await new Promise((resolve) => setTimeout(resolve, remaining));
        }
        const frame = path.join("samples", step.id, String(index).padStart(3, "0") + ".png");
        const elapsed_ms = Math.round(performance.now() - started);
        await screenshot(path.join(output, frame));
        clip.frames.push({ path: frame, target_ms, elapsed_ms });
    }
}

function describeError(error, phase) {
    if (phase.startsWith("step ")) {
        const message = String(error.message);
        if (message.includes("strict mode violation")) {
            return "selector matched multiple elements; target a unique control";
        }
        if (message.includes("intercepts pointer events")) {
            return "another element intercepted the pointer; dismiss the overlay or wait for it to close";
        }
        if (message.includes("not visible")) {
            return "element was hidden; open its container before interacting";
        }
        if (error.name === "TimeoutError") {
            return "timed out after " + timeout + " ms waiting for the target; check selector and page state";
        }
    }

    if (error.name === "TimeoutError") {
        return "operation timed out after " + timeout + " ms";
    }
    if (phase === "Playwright module setup" && error.code === "MODULE_NOT_FOUND") {
        return "Playwright is unavailable. Install it or supply --playwright-module.";
    }
    // Playwright errors can echo filled values and full URLs in their call logs.
    if (phase.startsWith("step ")) {
        return error.name;
    }
    if (phase === "initial navigation" && error.name !== "NavigationStatus") {
        return /net::ERR_[A-Z_]+/.exec(error.message)?.[0] ?? "navigation failed";
    }
    return String(error.message).split("\n")[0];
}

async function run() {
    let browser;
    let phase = "Playwright module setup";
    try {
        const { chromium } = require(modulePath ?? "playwright");
        phase = "Chromium launch";
        browser = await chromium.launch({ headless: true, timeout });
        result.browser = await browser.version();
        phase = "browser profile setup";
        const profile = scenario.profile;
        const context = await browser.newContext({
            viewport: { width: profile.width, height: profile.height },
            deviceScaleFactor: profile.scale,
            colorScheme: profile.color_scheme,
            reducedMotion: profile.reduced_motion ? "reduce" : "no-preference",
            locale: profile.locale,
            acceptDownloads: false,
        });
        const page = await context.newPage();
        page.setDefaultTimeout(timeout);
        phase = "initial navigation";
        const response = await page.goto(scenario.url, { waitUntil: "load" });
        if (!response?.ok()) {
            const error = new Error("HTTP " + (response?.status() ?? "unknown"));
            error.name = "NavigationStatus";
            throw error;
        }
        for (const [index, step] of scenario.steps.entries()) {
            phase = "step " + (index + 1) + " (" + step.kind + ")";
            if (step.kind === "checkpoint") {
                const file = path.join("checkpoints", step.id + ".png");
                await page.screenshot({ path: path.join(output, file), timeout });
                result.checkpoints.push({ id: step.id, path: file });
            } else if (step.kind === "clip") {
                await record(page, step);
            } else {
                await action(page, step);
            }
        }
    } catch (error) {
        result.blocker = phase + ": " + describeError(error, phase);
    } finally {
        await browser?.close();
        process.stdout.write(JSON.stringify(result));
    }
}

run().catch((error) => {
    process.stderr.write(error.name + "\n");
    process.exitCode = 1;
});
