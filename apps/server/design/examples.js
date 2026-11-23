const pages = [...document.querySelectorAll("[data-example]")];
const navigation = [...document.querySelectorAll(".preview-nav a")];

function showExample(focus) {
  const fragment = document.getElementById(location.hash.slice(1));
  const scene = fragment?.closest("[data-example]") || el("components");
  for (const page of pages) page.hidden = page !== scene;
  for (const link of navigation) {
    if (link.getAttribute("href") === "#" + scene.id) link.setAttribute("aria-current", "page");
    else link.removeAttribute("aria-current");
  }
  if (scene.id !== "evidence") selectMode("side");
  if (!focus) return;
  if (fragment && fragment !== scene) fragment.scrollIntoView();
  else scene.querySelector("h1").focus();
}

window.addEventListener("hashchange", () => showExample(true));
showExample(false);

el("sample-primary").addEventListener("click", () => {
  el("button-feedback").textContent = "Example saved.";
  el("button-feedback").hidden = false;
});
el("sample-secondary").addEventListener("click", () => { el("button-feedback").hidden = true; });

const queueRows = [...document.querySelectorAll("#queue tbody tr")];
function filterQueue() {
  const query = el("queue-search").value.trim().toLowerCase();
  const status = el("queue-status").value;
  let visible = 0;
  for (const row of queueRows) {
    const matchesStatus = !status || row.dataset.state === status;
    const matchesName = row.querySelector("th").textContent.toLowerCase().includes(query);
    row.hidden = !(matchesStatus && matchesName);
    if (!row.hidden) visible += 1;
  }
  el("queue-table").hidden = visible === 0;
  el("queue-empty").hidden = visible !== 0;
  el("queue-count").textContent = visible + (visible === 1 ? " capture" : " captures");
}
el("queue-search").addEventListener("input", filterQueue);
el("queue-status").addEventListener("change", filterQueue);
el("clear-queue").addEventListener("click", () => {
  el("queue-search").value = "";
  el("queue-status").value = "";
  filterQueue();
  el("queue-search").focus();
});

const digestEnabled = el("digest-enabled");
const email = el("digest-email");
function updateDigest() {
  el("digest-fields").hidden = !digestEnabled.checked;
  email.required = digestEnabled.checked;
  email.disabled = !digestEnabled.checked;
}
digestEnabled.addEventListener("change", updateDigest);
el("settings-form").addEventListener("submit", event => {
  event.preventDefault();
  const name = el("workspace-name").value.trim();
  if (!name) {
    el("workspace-name").setCustomValidity("Enter a workspace name.");
    el("workspace-name").reportValidity();
    return;
  }
  el("preview-brand").textContent = name;
  document.body.dataset.density = el("evidence-density").value;
  el("settings-result").textContent = "Settings saved in this example.";
  el("settings-result").hidden = false;
});
el("workspace-name").addEventListener("input", event => event.target.setCustomValidity(""));
el("settings-form").addEventListener("reset", () => setTimeout(() => {
  el("workspace-name").setCustomValidity("");
  el("preview-brand").textContent = el("workspace-name").defaultValue;
  delete document.body.dataset.density;
  el("settings-result").hidden = true;
  updateDigest();
}));
